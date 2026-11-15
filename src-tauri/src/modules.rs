//! Module generation: prompt from the project structure, LLM call, validation, cache.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::parse::{ProjectStructure, analyze_project};
use crate::scan::{Project, scan_repo};
use crate::settings::load_llm_config;
use crate::store::{self, Module, ModuleFile, ModulesFile};

/// tokens ≈ chars / 4. The model context size is unknown, so this is a fixed budget.
// ponytail: fixed budget; make it a setting if users hit it.
const MAX_PROMPT_TOKENS: usize = 100_000;

static CANCEL: AtomicBool = AtomicBool::new(false);

const SYSTEM_PROMPT: &str = r#"You split a software project into logical modules.
Input: one line per source file: `path | imports: ... | symbols: ...`. Imports are other files of the same project.
Rules:
- Group files by responsibility, not only by folder. Use 3 to 15 modules.
- Every file must be in exactly one module with "primary": true. A file may also appear in other modules with "primary": false.
- Use only the file paths from the input, exactly as written.
Answer with JSON only, in this shape:
{"modules":[{"id":"billing","name":"Billing","purpose":"One short sentence.","files":[{"path":"src/a.ts","primary":true}]}]}"#;

#[derive(Serialize, Clone)]
struct Progress {
    stage: &'static str,
}

fn progress(app: &AppHandle, stage: &'static str) {
    let _ = app.emit("modules-progress", Progress { stage });
}

#[derive(Deserialize)]
struct Response {
    modules: Vec<Module>,
}

fn user_prompt(structure: &ProjectStructure, with_symbols: bool) -> String {
    let mut text = String::new();
    for file in &structure.files {
        text.push_str(&file.path);
        text.push_str(" | imports: ");
        text.push_str(&file.imports.join(", "));
        if with_symbols {
            let symbols: Vec<String> = file
                .symbols
                .iter()
                .map(|s| format!("{}({})", s.name, serde_json::to_string(&s.kind).unwrap_or_default().trim_matches('"')))
                .collect();
            text.push_str(" | symbols: ");
            text.push_str(&symbols.join(", "));
        }
        text.push('\n');
    }
    text
}

/// Full prompt; drops symbols when too large; errors when still too large.
fn build_prompt(structure: &ProjectStructure) -> Result<String, String> {
    for with_symbols in [true, false] {
        let prompt = user_prompt(structure, with_symbols);
        if (prompt.len() + SYSTEM_PROMPT.len()) / 4 <= MAX_PROMPT_TOKENS {
            return Ok(prompt);
        }
    }
    Err(format!(
        "The project is too large for one request ({} files). Batch split is not available yet.",
        structure.files.len()
    ))
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "module".into() } else { s }
}

/// Parses the model answer and enforces the invariants:
/// unknown paths dropped, every file has exactly one primary module,
/// files without a module go to "Unassigned".
fn validate(answer: &str, project_files: &[String]) -> Result<Vec<Module>, String> {
    // Tolerate text or code fences around the JSON object.
    let json = match (answer.find('{'), answer.rfind('}')) {
        (Some(a), Some(b)) if a < b => &answer[a..=b],
        _ => answer,
    };
    let mut modules = serde_json::from_str::<Response>(json)
        .map_err(|e| format!("The model did not return the expected JSON: {e}"))?
        .modules;
    if modules.is_empty() {
        return Err("The model returned no modules.".into());
    }

    let known: HashSet<&str> = project_files.iter().map(String::as_str).collect();
    let mut primary_seen: HashSet<String> = HashSet::new();
    let mut ids: HashSet<String> = HashSet::new();
    for module in &mut modules {
        let mut in_module = HashSet::new();
        module.files.retain(|f| known.contains(f.path.as_str()) && in_module.insert(f.path.clone()));
        for file in &mut module.files {
            if file.primary && !primary_seen.insert(file.path.clone()) {
                file.primary = false; // second primary claim: first wins
            }
        }
        let base = if module.id.trim().is_empty() { slug(&module.name) } else { slug(&module.id) };
        let mut id = base.clone();
        let mut n = 2;
        while !ids.insert(id.clone()) {
            id = format!("{base}-{n}");
            n += 1;
        }
        module.id = id;
    }
    // A file with no primary claim gets its first module as primary.
    for module in &mut modules {
        for file in &mut module.files {
            if !file.primary && primary_seen.insert(file.path.clone()) {
                file.primary = true;
            }
        }
    }
    modules.retain(|m| !m.files.is_empty());

    let assigned: HashSet<&str> = modules.iter().flat_map(|m| m.files.iter().map(|f| f.path.as_str())).collect();
    let leftover: Vec<ModuleFile> = project_files
        .iter()
        .filter(|p| !assigned.contains(p.as_str()))
        .map(|p| ModuleFile { path: p.clone(), primary: true })
        .collect();
    if !leftover.is_empty() {
        modules.push(Module {
            id: "unassigned".into(),
            name: "Unassigned".into(),
            purpose: "Files the model did not place in a module.".into(),
            files: leftover,
        });
    }
    Ok(modules)
}

async fn wait_cancel() {
    // ponytail: polling; use a Notify if 200 ms latency matters.
    while !CANCEL.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn find_project(repo: &Path, project_id: &str) -> Result<Project, String> {
    scan_repo(repo)?
        .projects
        .into_iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("Project not found: {project_id}"))
}

#[tauri::command]
pub async fn load_modules(repo_path: String, project_id: String) -> Result<Option<ModulesFile>, String> {
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
    .map_err(|e| format!("Loading did not finish: {e}"))?
}

#[tauri::command]
pub fn cancel_generation() {
    CANCEL.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn generate_modules(app: AppHandle, repo_path: String, project_id: String) -> Result<ModulesFile, String> {
    CANCEL.store(false, Ordering::SeqCst);
    let cfg = load_llm_config(&app)?;

    progress(&app, "Scanning");
    let handle = app.clone();
    let repo_dir = repo_path.clone();
    let (project, structure, cache_key) = tauri::async_runtime::spawn_blocking(move || {
        let repo = Path::new(&repo_path);
        let project = find_project(repo, &project_id)?;
        progress(&handle, "Parsing");
        let structure = analyze_project(repo, &project.files)?;
        let key = store::cache_key(repo, &project);
        Ok::<_, String>((project, structure, key))
    })
    .await
    .map_err(|e| format!("Scanning did not finish: {e}"))??;
    if project.files.is_empty() {
        return Err("The project has no source files.".into());
    }
    let prompt = build_prompt(&structure)?;

    let mut last_error = String::new();
    for attempt in 0..2 {
        progress(&app, if attempt == 0 { "Calling model" } else { "Retrying" });
        let answer = tokio::select! {
            r = crate::llm::complete(&cfg, SYSTEM_PROMPT, &prompt, true) => r?,
            _ = wait_cancel() => return Err("Cancelled".into()),
        };
        progress(&app, "Validating");
        match validate(&answer, &project.files) {
            Ok(modules) => {
                let file = ModulesFile {
                    project: project.name.clone(),
                    cache_key,
                    model: cfg.model.clone(),
                    generated_at: store::now_rfc3339(),
                    modules,
                    outdated: false,
                };
                store::save(Path::new(&repo_dir), &project.id, &file)?;
                return Ok(file);
            }
            Err(e) => last_error = e,
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn paths(m: &Module) -> Vec<(&str, bool)> {
        m.files.iter().map(|f| (f.path.as_str(), f.primary)).collect()
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
        assert_eq!((last.name.as_str(), paths(last)), ("Unassigned", vec![("y.ts", true)]));
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
        let primaries = |p: &str| modules.iter().flat_map(|m| &m.files).filter(|f| f.path == p && f.primary).count();
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
                    symbols: (0..symbols).map(|s| Symbol { name: format!("sym{s}xxxxxxxxxxxxxxxxxxxx"), kind: SymbolKind::Class }).collect(),
                })
                .collect(),
        };
        // Fits only without symbols.
        let prompt = build_prompt(&big(5_000, 40)).unwrap();
        assert!(!prompt.contains("symbols:"));
        assert!(build_prompt(&big(20_000, 0)).unwrap_err().contains("20000 files"));
    }
}
