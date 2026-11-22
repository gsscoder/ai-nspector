//! Repo-specific cache in `<repo>/.ainspector/`: one JSON file per project, plus the cache key.
//! Never stores secrets (API key, base URL).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::AppError;
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
    /// Seconds from the start of the generation to the save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
}

/// FNV-1a: stable across Rust versions (unlike `DefaultHasher`), so a stored key stays valid.
fn fnv<S: AsRef<str>>(parts: impl IntoIterator<Item = S>) -> String {
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
    use crate::scan::Language;

    fn project(files: &[&str]) -> Project {
        Project {
            id: "package.json".into(),
            name: "demo".into(),
            language: Language::Typescript,
            languages: vec![Language::Typescript],
            role: crate::scan::Role::Unknown,
            frameworks: vec![],
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
            duration_secs: Some(12),
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
        assert_eq!(loaded.duration_secs, Some(12));
    }

    #[test]
    fn a_file_without_duration_secs_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(modules_dir(dir.path())).unwrap();
        fs::write(
            modules_dir(dir.path()).join(file_name("package.json")),
            r#"{"project":"demo","cacheKey":"fp:1","model":"m","generatedAt":"2023-11-14T22:13:20Z","modules":[]}"#,
        )
        .unwrap();

        let loaded = load(dir.path(), "package.json").unwrap().unwrap();

        assert_eq!(loaded.duration_secs, None);
    }

    #[test]
    fn a_missing_duration_secs_is_not_serialized() {
        let file = ModulesFile {
            project: "demo".into(),
            cache_key: "fp:1".into(),
            model: "m".into(),
            generated_at: "2023-11-14T22:13:20Z".into(),
            modules: vec![],
            outdated: false,
            duration_secs: None,
        };

        let json = serde_json::to_string(&file).unwrap();

        assert!(!json.contains("durationSecs"), "{json}");
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
