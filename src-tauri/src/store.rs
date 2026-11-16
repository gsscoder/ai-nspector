//! Repo-specific cache in `<repo>/.ainspector/`: one JSON file per project, one chat file per
//! module, plus the cache key. Never stores secrets (API key, base URL).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::llm::ChatMessage;
use crate::scan::Project;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleFile {
    pub path: String,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Module {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub purpose: String,
    pub files: Vec<ModuleFile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModulesFile {
    pub project: String,
    pub cache_key: String,
    pub model: String,
    /// RFC 3339, UTC.
    pub generated_at: String,
    pub modules: Vec<Module>,
    /// Computed on load; always stored as `false`.
    #[serde(default)]
    pub outdated: bool,
}

/// The analysis chat of one module. It holds only what the user sees: the context of each
/// model call (system prompt, overview, structure, source) is built again and never stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatFile {
    /// Id of the analysis that wrote the first message.
    pub analysis: String,
    pub module_name: String,
    /// Hash of the path and text of each module file: it detects a changed module.
    pub content_key: String,
    pub model: String,
    /// RFC 3339, UTC.
    pub created_at: String,
    pub messages: Vec<ChatMessage>,
    /// Computed on load; always stored as `false`.
    #[serde(default)]
    pub outdated: bool,
}

/// FNV-1a: stable across Rust versions (unlike `DefaultHasher`), so a stored key stays valid.
pub(crate) fn fnv<S: AsRef<str>>(parts: impl IntoIterator<Item = S>) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for part in parts {
        for byte in part.as_ref().bytes().chain([0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    format!("{hash:016x}")
}

pub fn store_dir(repo: &Path) -> PathBuf {
    repo.join(".ainspector")
}

fn modules_dir(repo: &Path) -> PathBuf {
    store_dir(repo).join("modules")
}

/// Creates `<repo>/.ainspector/` and, only when absent, its `.gitignore` with `*`.
pub fn ensure_store_dir(repo: &Path) -> Result<PathBuf, AppError> {
    let dir = store_dir(repo);
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::Io(format!("Cannot write to {}", dir.display()), e))?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        fs::write(&ignore, "*\n")
            .map_err(|e| AppError::Io(format!("Cannot write {}", ignore.display()), e))?;
    }
    Ok(dir)
}

fn slug_and_hash(id: &str) -> String {
    let slug: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    // The hash keeps `a/b` and `a_b` apart.
    format!("{slug}-{}", &fnv([id])[..8])
}

fn file_name(project_id: &str) -> String {
    format!("{}.json", slug_and_hash(project_id))
}

/// All chats of a project; the folder has the name of the modules file without `.json`.
fn chats_dir(repo: &Path, project_id: &str) -> PathBuf {
    store_dir(repo)
        .join("chats")
        .join(slug_and_hash(project_id))
}

fn chat_path(repo: &Path, project_id: &str, module_id: &str) -> PathBuf {
    chats_dir(repo, project_id).join(format!("{}.json", slug_and_hash(module_id)))
}

/// Stores `file` with `outdated` reset to `false`.
pub fn save(repo: &Path, project_id: &str, file: &mut ModulesFile) -> Result<(), AppError> {
    let dir = modules_dir(repo);
    let path = dir.join(file_name(project_id));
    ensure_store_dir(repo)?;
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::Io(format!("Cannot write to {}", dir.display()), e))?;
    file.outdated = false;
    let json = serde_json::to_string_pretty(file)
        .map_err(|e| AppError::Json("Cannot serialize the modules".into(), e))?;
    fs::write(&path, json).map_err(|e| AppError::Io(format!("Cannot write {}", path.display()), e))
}

pub fn load(repo: &Path, project_id: &str) -> Result<Option<ModulesFile>, AppError> {
    let path = modules_dir(repo).join(file_name(project_id));
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Io(format!("Cannot read {}", path.display()), e)),
    };
    serde_json::from_str(&text).map(Some).map_err(|e| {
        AppError::Json(
            format!("{} is damaged, regenerate the modules", path.display()),
            e,
        )
    })
}

/// Stores `file` with `outdated` reset to `false`. Writes a temp file and renames it, so a
/// failed write leaves the old chat untouched.
pub fn save_chat(
    repo: &Path,
    project_id: &str,
    module_id: &str,
    file: &mut ChatFile,
) -> Result<(), AppError> {
    let dir = chats_dir(repo, project_id);
    let path = chat_path(repo, project_id, module_id);
    let temp = path.with_extension("json.tmp");
    ensure_store_dir(repo)?;
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::Io(format!("Cannot write to {}", dir.display()), e))?;
    file.outdated = false;
    let json = serde_json::to_string_pretty(file)
        .map_err(|e| AppError::Json("Cannot serialize the chat".into(), e))?;
    fs::write(&temp, json)
        .map_err(|e| AppError::Io(format!("Cannot write {}", temp.display()), e))?;
    fs::rename(&temp, &path)
        .map_err(|e| AppError::Io(format!("Cannot write {}", path.display()), e))
}

pub fn load_chat(
    repo: &Path,
    project_id: &str,
    module_id: &str,
) -> Result<Option<ChatFile>, AppError> {
    let path = chat_path(repo, project_id, module_id);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Io(format!("Cannot read {}", path.display()), e)),
    };
    serde_json::from_str(&text).map(Some).map_err(|e| {
        AppError::Json(
            format!("{} is damaged, discard the chat", path.display()),
            e,
        )
    })
}

fn ignore_missing(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Deletes the chat of one module; a missing chat is not an error.
pub fn delete_chat(repo: &Path, project_id: &str, module_id: &str) -> Result<(), AppError> {
    let path = chat_path(repo, project_id, module_id);
    ignore_missing(fs::remove_file(&path))
        .map_err(|e| AppError::Io(format!("Cannot delete {}", path.display()), e))
}

/// Deletes every chat of a project: module ids can change when the modules are generated again.
pub fn delete_project_chats(repo: &Path, project_id: &str) -> Result<(), AppError> {
    let dir = chats_dir(repo, project_id);
    ignore_missing(fs::remove_dir_all(&dir))
        .map_err(|e| AppError::Io(format!("Cannot delete {}", dir.display()), e))
}

/// Runs `git` in `dir`; `None` when git is missing or the command fails.
/// The repo may be untrusted: its config must not run commands (`core.fsmonitor`) and the
/// index must not be rewritten.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false"])
        .args(args);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000); // no console window
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Hash of path, size and modified time of each file (relative to `base`).
fn fingerprint<'a>(base: &Path, files: impl Iterator<Item = &'a str>) -> String {
    let mut parts: Vec<String> = files
        .map(|rel| {
            let meta = fs::metadata(base.join(rel)).ok();
            let size = meta.as_ref().map_or(0, |m| m.len());
            let mtime = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{rel}|{size}|{mtime}")
        })
        .collect();
    parts.sort();
    fnv(parts)
}

/// `git:<HEAD>`, `git:<HEAD>+fp:<changed files>` or `fp:<all project files>`.
pub fn cache_key(repo: &Path, project: &Project) -> String {
    let dir = repo.join(&project.root);
    if let Some(head) = git(repo, &["rev-parse", "HEAD"]) {
        let head = head.trim();
        // Paths relative to the project folder. Staged, unstaged and untracked changes.
        // `-z` keeps non-ASCII names unquoted.
        let changed = git(
            &dir,
            &["diff", "-z", "--relative", "--name-only", "HEAD", "--", "."],
        )
        .zip(git(
            &dir,
            &["ls-files", "-z", "-o", "--exclude-standard", "--", "."],
        ));
        if let Some((tracked, untracked)) = changed {
            let all = format!("{tracked}{untracked}");
            let paths: Vec<&str> = all.split('\u{0}').filter(|l| !l.is_empty()).collect();
            return if paths.is_empty() {
                format!("git:{head}")
            } else {
                format!("git:{head}+fp:{}", fingerprint(&dir, paths.into_iter()))
            };
        }
    }
    format!(
        "fp:{}",
        fingerprint(repo, project.files.iter().map(String::as_str))
    )
}

pub fn now_rfc3339() -> String {
    rfc3339(SystemTime::now())
}

/// `time` as RFC 3339, UTC (no date crate needed).
pub fn rfc3339(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Role;
    use crate::scan::Language;

    fn project(files: &[&str]) -> Project {
        Project {
            id: "package.json".into(),
            name: "demo".into(),
            language: Language::Typescript,
            role: crate::scan::Role::Unknown,
            root: String::new(),
            files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    fn git_ok(dir: &Path, args: &[&str]) -> bool {
        git(dir, args).is_some()
    }

    #[test]
    fn save_creates_gitignore_and_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = ModulesFile {
            project: "demo".into(),
            cache_key: "fp:1".into(),
            model: "m".into(),
            generated_at: now_rfc3339(),
            modules: vec![Module {
                id: "a".into(),
                name: "A".into(),
                purpose: "p".into(),
                files: vec![ModuleFile {
                    path: "a.ts".into(),
                    primary: true,
                }],
            }],
            outdated: true,
        };
        assert_eq!(load(dir.path(), "package.json").unwrap(), None);
        save(dir.path(), "package.json", &mut file).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join(".ainspector/.gitignore")).unwrap(),
            "*\n"
        );
        let loaded = load(dir.path(), "package.json").unwrap().unwrap();
        assert_eq!(loaded, file);
        assert!(!loaded.outdated);
    }

    fn chat(analysis: &str) -> ChatFile {
        ChatFile {
            analysis: analysis.into(),
            module_name: "Billing".into(),
            content_key: "fnv:1".into(),
            model: "m".into(),
            created_at: "2026-11-16T10:00:00Z".into(),
            messages: vec![
                ChatMessage::new(Role::Assistant, "findings"),
                ChatMessage::new(Role::User, "why?"),
            ],
            outdated: true,
        }
    }

    #[test]
    fn chat_roundtrips_with_outdated_stored_as_false() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = chat("over-engineering");
        assert_eq!(
            load_chat(dir.path(), "package.json", "billing").unwrap(),
            None
        );

        save_chat(dir.path(), "package.json", "billing", &mut file).unwrap();

        assert_eq!(
            fs::read_to_string(dir.path().join(".ainspector/.gitignore")).unwrap(),
            "*\n"
        );
        let loaded = load_chat(dir.path(), "package.json", "billing")
            .unwrap()
            .unwrap();
        assert_eq!(loaded, file);
        assert!(!loaded.outdated);
    }

    #[test]
    fn chat_file_is_camel_case_with_lowercase_roles() {
        let json = serde_json::to_value(chat("over-engineering")).unwrap();
        assert_eq!(json["moduleName"], "Billing");
        assert_eq!(json["contentKey"], "fnv:1");
        assert_eq!(json["createdAt"], "2026-11-16T10:00:00Z");
        assert_eq!(json["messages"][0]["role"], "assistant");
        assert_eq!(json["messages"][1]["content"], "why?");
    }

    #[test]
    fn chat_is_stored_in_a_folder_per_project_and_a_file_per_module() {
        let dir = tempfile::tempdir().unwrap();
        save_chat(dir.path(), "web/package.json", "billing", &mut chat("a")).unwrap();

        let project_dir = chats_dir(dir.path(), "web/package.json");
        assert_eq!(
            project_dir.parent().unwrap(),
            dir.path().join(".ainspector/chats")
        );
        let modules_file = file_name("web/package.json");
        assert_eq!(
            project_dir.file_name().unwrap().to_str().unwrap(),
            modules_file.strip_suffix(".json").unwrap()
        );
        assert!(chat_path(dir.path(), "web/package.json", "billing").is_file());
        // Only the chat: the temp file is gone.
        assert_eq!(fs::read_dir(&project_dir).unwrap().count(), 1);
    }

    #[test]
    fn ids_with_the_same_slug_get_different_chat_files() {
        let dir = tempfile::tempdir().unwrap();
        assert_ne!(
            chat_path(dir.path(), "p", "a/b"),
            chat_path(dir.path(), "p", "a_b")
        );
    }

    #[test]
    fn a_damaged_chat_names_its_path_and_can_be_deleted() {
        let dir = tempfile::tempdir().unwrap();
        save_chat(dir.path(), "p", "m", &mut chat("a")).unwrap();
        let path = chat_path(dir.path(), "p", "m");
        fs::write(&path, "not json").unwrap();

        let error = load_chat(dir.path(), "p", "m").unwrap_err().to_string();
        assert!(error.contains("damaged"), "{error}");

        delete_chat(dir.path(), "p", "m").unwrap();
        assert_eq!(load_chat(dir.path(), "p", "m").unwrap(), None);
    }

    #[test]
    fn deleting_one_chat_keeps_the_others_and_a_missing_chat_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        save_chat(dir.path(), "p", "one", &mut chat("a")).unwrap();
        save_chat(dir.path(), "p", "two", &mut chat("a")).unwrap();

        delete_chat(dir.path(), "p", "one").unwrap();
        delete_chat(dir.path(), "p", "one").unwrap();
        delete_chat(dir.path(), "never-saved", "one").unwrap();

        assert_eq!(load_chat(dir.path(), "p", "one").unwrap(), None);
        assert!(load_chat(dir.path(), "p", "two").unwrap().is_some());
    }

    #[test]
    fn deleting_the_chats_of_a_project_keeps_other_projects_and_the_modules_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut modules = ModulesFile {
            project: "demo".into(),
            cache_key: "fp:1".into(),
            model: "m".into(),
            generated_at: now_rfc3339(),
            modules: vec![],
            outdated: false,
        };
        save(dir.path(), "p", &mut modules).unwrap();
        save_chat(dir.path(), "p", "one", &mut chat("a")).unwrap();
        save_chat(dir.path(), "p", "two", &mut chat("a")).unwrap();
        save_chat(dir.path(), "other", "one", &mut chat("a")).unwrap();

        delete_project_chats(dir.path(), "p").unwrap();
        delete_project_chats(dir.path(), "p").unwrap();

        assert_eq!(load_chat(dir.path(), "p", "one").unwrap(), None);
        assert_eq!(load_chat(dir.path(), "p", "two").unwrap(), None);
        assert!(load_chat(dir.path(), "other", "one").unwrap().is_some());
        assert!(load(dir.path(), "p").unwrap().is_some());
    }

    #[test]
    fn a_failed_chat_write_leaves_the_old_chat_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let mut old = chat("old");
        save_chat(dir.path(), "p", "m", &mut old).unwrap();
        // A folder on the temp name makes the write fail.
        let path = chat_path(dir.path(), "p", "m");
        fs::create_dir(path.with_extension("json.tmp")).unwrap();

        assert!(save_chat(dir.path(), "p", "m", &mut chat("new")).is_err());

        assert_eq!(load_chat(dir.path(), "p", "m").unwrap().unwrap(), old);
    }

    #[test]
    fn rfc3339_shape() {
        let now = now_rfc3339();
        assert_eq!(now.len(), 20);
        assert!(now.starts_with("20") && now.ends_with('Z'), "{now}");
    }

    #[test]
    fn rfc3339_formats_a_given_time() {
        let time = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        assert_eq!(rfc3339(time), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn no_git_uses_fingerprint_and_detects_change() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.ts"), "1").unwrap();
        let p = project(&["a.ts"]);
        let key = cache_key(dir.path(), &p);
        assert!(key.starts_with("fp:"), "{key}");
        assert_eq!(key, cache_key(dir.path(), &p));
        fs::write(dir.path().join("a.ts"), "12345").unwrap();
        assert_ne!(key, cache_key(dir.path(), &p));
    }

    fn committed_repo(d: &Path) -> bool {
        if !git_ok(d, &["init", "-q"]) {
            return false; // git not installed
        }
        fs::write(d.join("a.ts"), "1").unwrap();
        assert!(git_ok(d, &["add", "."]));
        assert!(git_ok(
            d,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "x"
            ]
        ));
        true
    }

    #[test]
    fn git_key_changes_when_a_non_ascii_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        if !committed_repo(d) {
            return;
        }
        let p = project(&["a.ts", "módulo.ts"]);
        fs::write(d.join("módulo.ts"), "1").unwrap();
        let before = cache_key(d, &p);
        fs::write(d.join("módulo.ts"), "12345").unwrap();
        assert_ne!(before, cache_key(d, &p));
    }

    #[test]
    fn git_does_not_run_the_fsmonitor_command_of_the_repo() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        if !committed_repo(d) {
            return;
        }
        assert!(git_ok(
            d,
            &["config", "core.fsmonitor", "echo > fsmonitor-ran"]
        ));
        fs::write(d.join("a.ts"), "changed").unwrap();
        cache_key(d, &project(&["a.ts"]));
        assert!(!d.join("fsmonitor-ran").exists());
    }

    #[test]
    fn git_clean_then_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        if !git_ok(d, &["init", "-q"]) {
            return; // git not installed
        }
        fs::write(d.join("a.ts"), "1").unwrap();
        assert!(git_ok(d, &["add", "."]));
        assert!(git_ok(
            d,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "x"
            ]
        ));
        let p = project(&["a.ts"]);
        let clean = cache_key(d, &p);
        assert!(
            clean.starts_with("git:") && !clean.contains("+fp:"),
            "{clean}"
        );
        fs::write(d.join("a.ts"), "changed").unwrap();
        let dirty = cache_key(d, &p);
        assert!(
            dirty.starts_with(&clean) && dirty.contains("+fp:"),
            "{dirty}"
        );
    }
}
