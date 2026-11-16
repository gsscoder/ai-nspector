//! Module generation: prompt from the project structure, LLM call, validation, cache.

use std::collections::HashSet;
use std::mem;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::error::AppError;
use crate::parse::{ProjectStructure, analyze_project};
use crate::scan::{Project, scan_repo};
use crate::settings::load_llm_config;
use crate::store::{self, Module, ModuleFile, ModulesFile};

/// tokens ≈ chars / 4. The model context size is unknown, so this is a fixed budget.
// ponytail: fixed budget; make it a setting if users hit it.
pub(crate) const MAX_PROMPT_TOKENS: usize = 100_000;

pub(crate) static CANCEL: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Allows one generation at a time, so the shared `CANCEL` flag and the cache file belong to one run.
/// Held until dropped: released on return, early `?` and unwinding alike.
pub(crate) struct RunGuard;

impl RunGuard {
    pub(crate) fn acquire() -> Result<Self, AppError> {
        RUNNING
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| Self)
            .map_err(|_| AppError::Invalid("A generation is already running.".into()))
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

const SYSTEM_PROMPT: &str = r#"you split a software project into logical modules
input: one line per source file: `path | imports: ... | symbols: ...`. imports are other files of the same project
rules:
- group files by responsibility, not only by folder. use 3 to 15 modules
- every file must be in exactly one module with "primary": true. a file may also appear in other modules with "primary": false
- use only the file paths from the input, exactly as written
answer with JSON only, in this shape:
{"modules":[{"id":"billing","name":"Billing","purpose":"One short sentence.","files":[{"path":"src/a.ts","primary":true}]}]}"#;

#[derive(Serialize, Clone)]
struct Progress {
    stage: &'static str,
}

pub(crate) fn progress(app: &AppHandle, stage: &'static str) {
    let _ = app.emit("modules-progress", Progress { stage });
}

#[derive(Deserialize)]
struct Response {
    modules: Vec<Module>,
}

pub(crate) fn user_prompt(structure: &ProjectStructure, with_symbols: bool) -> String {
    let mut text = String::new();
    for file in &structure.files {
        text.push_str(&file.path);
        text.push_str(" | imports: ");
        text.push_str(&file.imports.join(", "));
        if with_symbols {
            let symbols: Vec<String> = file
                .symbols
                .iter()
                .map(|s| {
                    format!(
                        "{}({})",
                        s.name,
                        serde_json::to_string(&s.kind)
                            .unwrap_or_default()
                            .trim_matches('"')
                    )
                })
                .collect();
            text.push_str(" | symbols: ");
            text.push_str(&symbols.join(", "));
        }
        text.push('\n');
    }
    text
}

/// Full prompt; drops symbols when too large; errors when still too large.
fn build_prompt(structure: &ProjectStructure) -> Result<String, AppError> {
    for with_symbols in [true, false] {
        let prompt = user_prompt(structure, with_symbols);
        if (prompt.len() + SYSTEM_PROMPT.len()) / 4 <= MAX_PROMPT_TOKENS {
            return Ok(prompt);
        }
    }
    Err(AppError::Invalid(format!(
        "The project is too large for one request ({} files). Batch split is not available yet.",
        structure.files.len()
    )))
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if s.is_empty() { "module".into() } else { s }
}

/// Appends `-2`, `-3`, ... to `id` until it is not in `ids`, then records and returns it.
fn claim_unique_id(ids: &mut HashSet<String>, mut id: String) -> String {
    let base_len = id.len();
    let mut n = 2;
    while ids.contains(&id) {
        id.truncate(base_len);
        id.push_str(&format!("-{n}"));
        n += 1;
    }
    ids.insert(id.clone());
    id
}

/// Parses the model answer and enforces the invariants:
/// unknown paths dropped, every file has exactly one primary module,
/// files without a module go to "Unassigned".
fn validate(answer: &str, project_files: &[String]) -> Result<Vec<Module>, AppError> {
    // Tolerate text or code fences around the JSON object.
    let json = match (answer.find('{'), answer.rfind('}')) {
        (Some(a), Some(b)) if a < b => &answer[a..=b],
        _ => answer,
    };
    let mut modules = serde_json::from_str::<Response>(json)
        .map_err(|e| AppError::Json("The model did not return the expected JSON".into(), e))?
        .modules;
    if modules.is_empty() {
        return Err(AppError::Llm("The model returned no modules.".into()));
    }

    // Sets hold the `project_files` strings, so tracking a path never copies it.
    let known: HashSet<&str> = project_files.iter().map(String::as_str).collect();
    let mut primary_seen: HashSet<&str> = HashSet::new();
    let mut ids: HashSet<String> = HashSet::new();
    for module in &mut modules {
        let mut in_module = HashSet::new();
        module.files = mem::take(&mut module.files)
            .into_iter()
            .filter_map(|mut file| {
                let path = *known.get(file.path.as_str())?;
                if !in_module.insert(path) {
                    return None;
                }
                if file.primary && !primary_seen.insert(path) {
                    file.primary = false; // second primary claim: first wins
                }
                Some(file)
            })
            .collect();
        let id = if module.id.trim().is_empty() {
            slug(&module.name)
        } else {
            slug(&module.id)
        };
        module.id = claim_unique_id(&mut ids, id);
    }
    // A file with no primary claim gets its first module as primary.
    for module in &mut modules {
        for file in &mut module.files {
            if !file.primary
                && let Some(&path) = known.get(file.path.as_str())
                && primary_seen.insert(path)
            {
                file.primary = true;
            }
        }
    }
    modules.retain(|m| !m.files.is_empty());

    let assigned: HashSet<&str> = modules
        .iter()
        .flat_map(|m| m.files.iter().map(|f| f.path.as_str()))
        .collect();
    let leftover: Vec<ModuleFile> = project_files
        .iter()
        .filter(|p| !assigned.contains(p.as_str()))
        .map(|p| ModuleFile {
            path: p.clone(),
            primary: true,
        })
        .collect();
    if !leftover.is_empty() {
        modules.push(Module {
            id: claim_unique_id(&mut ids, "unassigned".into()),
            name: "Unassigned".into(),
            purpose: "Files the model did not place in a module.".into(),
            files: leftover,
        });
    }
    Ok(modules)
}

pub(crate) async fn wait_cancel() {
    // ponytail: polling; use a Notify if 200 ms latency matters.
    while !CANCEL.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub(crate) fn find_project(repo: &Path, project_id: &str) -> Result<Project, AppError> {
    scan_repo(repo)?
        .projects
        .into_iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| AppError::Invalid(format!("Project not found: {project_id}")))
}

#[tauri::command]
pub async fn load_modules(
    repo_path: String,
    project_id: String,
) -> Result<Option<ModulesFile>, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let repo = Path::new(&repo_path);
        let Some(mut file) = store::load(repo, &project_id)? else {
            return Ok(None);
        };
        let project = find_project(repo, &project_id)?;
        file.outdated = file.cache_key != store::cache_key(repo, &project);
        Ok(Some(file))
    })
    .await
    .map_err(|e| AppError::Platform("Loading did not finish".into(), e.into()))?
}

#[tauri::command]
pub fn cancel_generation() {
    CANCEL.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn generate_modules(
    app: AppHandle,
    repo_path: String,
    project_id: String,
) -> Result<ModulesFile, AppError> {
    let _run = RunGuard::acquire()?;
    CANCEL.store(false, Ordering::SeqCst);
    let cfg = load_llm_config(&app)?;

    progress(&app, "Scanning");
    let handle = app.clone();
    let repo_dir = repo_path.clone();
    let (project, structure, cache_key) = tauri::async_runtime::spawn_blocking(move || {
        let repo = Path::new(&repo_path);
        let project = find_project(repo, &project_id)?;
        progress(&handle, "Parsing");
        let structure = analyze_project(repo, &project.files).map_err(AppError::Invalid)?;
        let key = store::cache_key(repo, &project);
        Ok::<_, AppError>((project, structure, key))
    })
    .await
    .map_err(|e| AppError::Platform("Scanning did not finish".into(), e.into()))??;
    if project.files.is_empty() {
        return Err(AppError::Invalid("The project has no source files.".into()));
    }
    let prompt = build_prompt(&structure)?;

    let mut retried = false;
    loop {
        progress(&app, if retried { "Retrying" } else { "Calling model" });
        let answer = tokio::select! {
            r = crate::llm::complete(&cfg, SYSTEM_PROMPT, &prompt, true) => r?,
            _ = wait_cancel() => return Err(AppError::Cancelled),
        };
        progress(&app, "Validating");
        match validate(&answer, &project.files) {
            Ok(modules) => {
                let mut file = ModulesFile {
                    project: project.name,
                    cache_key,
                    model: cfg.model,
                    generated_at: store::now_rfc3339(),
                    modules,
                    outdated: false,
                };
                store::save(Path::new(&repo_dir), &project.id, &mut file)?;
                // Module ids can change, so the old chats could point to the wrong files.
                store::delete_project_chats(Path::new(&repo_dir), &project.id)?;
                return Ok(file);
            }
            Err(e) if retried => return Err(e),
            Err(_) => retried = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn paths(m: &Module) -> Vec<(&str, bool)> {
        m.files
            .iter()
            .map(|f| (f.path.as_str(), f.primary))
            .collect()
    }

    #[test]
    fn valid_answer_is_kept() {
        let json = r#"{"modules":[{"id":"a","name":"A","purpose":"p","files":[{"path":"x.ts","primary":true}]},
            {"id":"b","name":"B","purpose":"q","files":[{"path":"y.ts","primary":true},{"path":"x.ts","primary":false}]}]}"#;
        let modules = validate(json, &files(&["x.ts", "y.ts"])).unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(paths(&modules[1]), [("y.ts", true), ("x.ts", false)]);
    }

    #[test]
    fn missing_files_go_to_unassigned() {
        let json = r#"{"modules":[{"id":"a","name":"A","purpose":"p","files":[{"path":"x.ts","primary":true}]}]}"#;
        let modules = validate(json, &files(&["x.ts", "y.ts"])).unwrap();
        let last = modules.last().unwrap();
        assert_eq!(
            (last.name.as_str(), paths(last)),
            ("Unassigned", vec![("y.ts", true)])
        );
    }

    #[test]
    fn unassigned_id_is_unique_when_the_model_already_used_it() {
        let json = r#"{"modules":[{"id":"unassigned","name":"Mine","purpose":"p","files":[{"path":"x.ts","primary":true}]}]}"#;
        let modules = validate(json, &files(&["x.ts", "y.ts"])).unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].id, "unassigned");
        assert_eq!(
            (modules[1].name.as_str(), modules[1].id.as_str()),
            ("Unassigned", "unassigned-2")
        );
    }

    #[test]
    fn only_one_generation_runs_at_a_time() {
        let first = RunGuard::acquire().unwrap();
        assert_eq!(
            RunGuard::acquire().err().unwrap().to_string(),
            "A generation is already running."
        );
        drop(first);
        assert!(RunGuard::acquire().is_ok());
    }

    #[test]
    fn unknown_paths_are_dropped_and_empty_modules_removed() {
        let json = r#"{"modules":[{"id":"a","name":"A","purpose":"p","files":[{"path":"x.ts","primary":true},{"path":"ghost.ts","primary":true}]},
            {"id":"b","name":"B","purpose":"q","files":[{"path":"nope.ts","primary":true}]}]}"#;
        let modules = validate(json, &files(&["x.ts"])).unwrap();
        assert_eq!(modules.len(), 1);
        assert_eq!(paths(&modules[0]), [("x.ts", true)]);
    }

    #[test]
    fn exactly_one_primary_per_file() {
        // x.ts claimed primary twice; y.ts has no primary claim at all.
        let json = r#"{"modules":[{"id":"a","name":"A","files":[{"path":"x.ts","primary":true},{"path":"y.ts"}]},
            {"id":"a","name":"B","files":[{"path":"x.ts","primary":true}]}]}"#;
        let modules = validate(json, &files(&["x.ts", "y.ts"])).unwrap();
        let primaries = |p: &str| {
            modules
                .iter()
                .flat_map(|m| &m.files)
                .filter(|f| f.path == p && f.primary)
                .count()
        };
        assert_eq!((primaries("x.ts"), primaries("y.ts")), (1, 1));
        assert_ne!(modules[0].id, modules[1].id);
    }

    #[test]
    fn broken_json_and_empty_modules_are_errors() {
        assert!(validate("not json", &files(&["x.ts"])).is_err());
        assert!(validate(r#"{"modules":"nope"}"#, &files(&["x.ts"])).is_err());
        assert!(validate(r#"{"modules":[]}"#, &files(&["x.ts"])).is_err());
    }

    #[test]
    fn code_fences_are_tolerated() {
        let json = "```json
{\"modules\":[{\"id\":\"a\",\"name\":\"A\",\"files\":[{\"path\":\"x.ts\",\"primary\":true}]}]}
```";
        assert_eq!(validate(json, &files(&["x.ts"])).unwrap().len(), 1);
    }

    #[test]
    fn too_large_project_drops_symbols_then_errors() {
        use crate::parse::{FileInfo, Symbol, SymbolKind};
        let big = |n: usize, symbols: usize| ProjectStructure {
            files: (0..n)
                .map(|i| FileInfo {
                    path: format!("src/file{i}.ts"),
                    imports: vec![],
                    symbols: (0..symbols)
                        .map(|s| Symbol {
                            name: format!("sym{s}xxxxxxxxxxxxxxxxxxxx"),
                            kind: SymbolKind::Class,
                        })
                        .collect(),
                })
                .collect(),
        };
        // Fits only without symbols.
        let prompt = build_prompt(&big(5_000, 40)).unwrap();
        assert!(!prompt.contains("symbols:"));
        assert!(
            build_prompt(&big(20_000, 0))
                .unwrap_err()
                .to_string()
                .contains("20000 files")
        );
    }
}
