use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use ignore::WalkBuilder;
use serde::Serialize;
use tauri::Manager;

use crate::error::AppError;

const SKIPPED_DIRS: [&str; 8] = [
    "bin",
    "obj",
    "node_modules",
    ".venv",
    "dist",
    ".ainspector",
    "target",
    "__pycache__",
];
const MAX_MANIFEST_BYTES: u64 = 1_000_000;
// `next` and `nuxt` are in both lists: they serve a page and an API, so the role stays `Unknown`.
const FRONTEND_PACKAGES: [&str; 6] = ["react", "vue", "svelte", "@angular/core", "next", "nuxt"];
const BACKEND_PACKAGES: [&str; 7] = [
    "express",
    "fastify",
    "@nestjs/core",
    "hono",
    "koa",
    "next",
    "nuxt",
];
const BACKEND_PYTHON: [&str; 3] = ["fastapi", "flask", "django"];
const OTHER_PROJECT_ID: &str = "(other)";
const OTHER_PROJECT_NAME: &str = "(other files)";
const RECENT_REPOS_FILE: &str = "recent_repos.json";
const MAX_RECENT_REPOS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Python,
    Dotnet,
    Typescript,
    Other,
}

/// Read from the manifest content only; `Unknown` when the signals are absent or mixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Backend,
    Frontend,
    Unknown,
}

/// All paths are relative to the repo root and use forward slashes.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub language: Language,
    pub role: Role,
    /// Project folder; empty for the repo root.
    pub root: String,
    /// Source files owned by this project, sorted.
    pub files: Vec<String>,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub root: String,
    pub name: String,
    pub projects: Vec<Project>,
}

struct Manifest {
    id: String,
    name: String,
    dir: String,
    language: Language,
    role: Role,
    /// Lower wins when one folder holds several manifests of one language.
    priority: u8,
}

impl Manifest {
    fn new(
        path: String,
        file_name: &str,
        language: Language,
        role: Role,
        priority: u8,
        repo_name: &str,
    ) -> Self {
        let dir = parent_dir(&path).to_string();
        let name = match file_name.strip_suffix(".csproj") {
            Some(stem) => stem.to_string(),
            None if dir.is_empty() => repo_name.to_string(),
            None => dir.rsplit('/').next().unwrap_or(&dir).to_string(),
        };
        Manifest {
            id: path,
            name,
            dir,
            language,
            role,
            priority,
        }
    }

    fn into_project(self, files: Vec<String>) -> Project {
        Project {
            id: self.id,
            name: self.name,
            language: self.language,
            role: self.role,
            root: self.dir,
            files,
        }
    }
}

struct SourceFile {
    path: String,
    language: Language,
}

struct Walked {
    manifests: Vec<Manifest>,
    sources: Vec<SourceFile>,
}

pub fn scan_repo(root: &Path) -> Result<Repo, AppError> {
    if !root.is_dir() {
        return Err(AppError::Invalid(format!(
            "cannot open repo: {} is not a directory",
            root.display()
        )));
    }
    let name = root
        .file_name()
        .map_or_else(|| root.to_string_lossy(), |n| n.to_string_lossy())
        .into_owned();
    let walked = walk_repo(root, &name);
    Ok(Repo {
        root: root.to_string_lossy().into_owned(),
        projects: group_into_projects(walked.manifests, walked.sources),
        name,
    })
}

fn walk_repo(root: &Path, repo_name: &str) -> Walked {
    let walker = WalkBuilder::new(root)
        .require_git(false)
        .parents(false)
        .filter_entry(|entry| {
            let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
            entry.depth() == 0 || !is_dir || !SKIPPED_DIRS.iter().any(|d| entry.file_name() == *d)
        })
        .build();

    let mut walked = Walked {
        manifests: Vec::new(),
        sources: Vec::new(),
    };
    for entry in walker {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let path = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let file_name = entry.file_name().to_string_lossy();
        match (manifest_kind(&file_name), source_language(&file_name)) {
            (Some((language, priority)), source) => {
                // `setup.py` is both a manifest and a source file.
                let source_file = source.map(|language| SourceFile {
                    path: path.clone(),
                    language,
                });
                let role = detect_role(&file_name, &read_manifest(entry.path()));
                walked.manifests.push(Manifest::new(
                    path, &file_name, language, role, priority, repo_name,
                ));
                walked.sources.extend(source_file);
            }
            (None, Some(language)) => walked.sources.push(SourceFile { path, language }),
            (None, None) => {}
        }
    }
    walked
}

fn manifest_kind(file_name: &str) -> Option<(Language, u8)> {
    match file_name {
        "pyproject.toml" => Some((Language::Python, 0)),
        "setup.py" => Some((Language::Python, 1)),
        "requirements.txt" => Some((Language::Python, 2)),
        "package.json" => Some((Language::Typescript, 0)),
        _ if file_name.ends_with(".csproj") => Some((Language::Dotnet, 0)),
        _ => None,
    }
}

fn read_manifest(path: &Path) -> String {
    fs::metadata(path)
        .ok()
        .filter(|m| m.len() <= MAX_MANIFEST_BYTES)
        .and_then(|_| fs::read_to_string(path).ok())
        .unwrap_or_default()
}

// ponytail: plain name match, no version or script check; a mixed signal gives `Unknown`.
fn detect_role(file_name: &str, text: &str) -> Role {
    let (frontend, backend) = match file_name {
        "package.json" => {
            let json: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
            let has = |names: &[&str]| {
                ["dependencies", "devDependencies"]
                    .iter()
                    .any(|section| names.iter().any(|name| json[*section].get(*name).is_some()))
            };
            (has(&FRONTEND_PACKAGES), has(&BACKEND_PACKAGES))
        }
        "pyproject.toml" | "setup.py" | "requirements.txt" => {
            let text = text.to_lowercase();
            (false, BACKEND_PYTHON.iter().any(|name| text.contains(name)))
        }
        _ if file_name.ends_with(".csproj") => (
            text.contains("Microsoft.NET.Sdk.BlazorWebAssembly\""),
            text.contains("Microsoft.NET.Sdk.Web\""),
        ),
        _ => (false, false),
    };
    match (frontend, backend) {
        (true, false) => Role::Frontend,
        (false, true) => Role::Backend,
        _ => Role::Unknown,
    }
}

fn source_language(file_name: &str) -> Option<Language> {
    match file_name.rsplit_once('.')?.1 {
        "py" => Some(Language::Python),
        "cs" => Some(Language::Dotnet),
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Some(Language::Typescript),
        _ => None,
    }
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

/// A project exists only if it owns at least one source file.
fn group_into_projects(manifests: Vec<Manifest>, mut sources: Vec<SourceFile>) -> Vec<Project> {
    let mut by_location: BTreeMap<(&str, Language), (usize, &Manifest)> = BTreeMap::new();
    for (index, manifest) in manifests.iter().enumerate() {
        by_location
            .entry((manifest.dir.as_str(), manifest.language))
            .and_modify(|best| {
                if (manifest.priority, &manifest.id) < (best.1.priority, &best.1.id) {
                    *best = (index, manifest);
                }
            })
            .or_insert((index, manifest));
    }

    sources.sort_by(|a, b| a.path.cmp(&b.path));
    let mut files_by_manifest: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    let mut other_files = Vec::new();
    for source in sources {
        match nearest_manifest(&by_location, &source) {
            Some(index) => files_by_manifest
                .entry(index)
                .or_default()
                .push(source.path),
            None => other_files.push(source.path),
        }
    }

    let mut projects: Vec<Project> = manifests
        .into_iter()
        .enumerate()
        .filter_map(|(index, manifest)| {
            files_by_manifest
                .remove(&index)
                .map(|files| manifest.into_project(files))
        })
        .collect();
    projects.sort_by(|a, b| a.id.cmp(&b.id));
    if !other_files.is_empty() {
        projects.push(Project {
            id: OTHER_PROJECT_ID.to_string(),
            name: OTHER_PROJECT_NAME.to_string(),
            language: Language::Other,
            role: Role::Unknown,
            root: String::new(),
            files: other_files,
        });
    }
    projects
}

/// Index in the manifest list of the manifest that owns `source`.
fn nearest_manifest(
    by_location: &BTreeMap<(&str, Language), (usize, &Manifest)>,
    source: &SourceFile,
) -> Option<usize> {
    let mut dir = parent_dir(&source.path);
    loop {
        if let Some((index, _)) = by_location.get(&(dir, source.language)) {
            return Some(*index);
        }
        if dir.is_empty() {
            return None;
        }
        dir = parent_dir(dir);
    }
}

fn read_recent_repos(file: &Path) -> Result<Vec<String>, AppError> {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(AppError::Io(format!("cannot read {}", file.display()), e)),
    };
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

/// Puts `repo` first, removes its older entry, and keeps at most `MAX_RECENT_REPOS` entries.
fn add_recent_repo(file: &Path, repo: &str) -> Result<(), AppError> {
    let mut recent = read_recent_repos(file)?;
    recent.retain(|existing| existing != repo);
    recent.insert(0, repo.to_string());
    recent.truncate(MAX_RECENT_REPOS);

    let text = serde_json::to_string(&recent)
        .map_err(|e| AppError::Json(format!("cannot write {}", file.display()), e))?;
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)
            .map_err(|e| AppError::Io(format!("cannot create {}", folder.display()), e))?;
    }
    let temp = file.with_extension("json.tmp");
    fs::write(&temp, text)
        .and_then(|()| fs::rename(&temp, file))
        .map_err(|e| AppError::Io(format!("cannot write {}", file.display()), e))
}

fn recent_repos_file(app: &tauri::AppHandle) -> Result<std::path::PathBuf, AppError> {
    let folder = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Platform("cannot find the app data folder".into(), e.into()))?;
    Ok(folder.join(RECENT_REPOS_FILE))
}

#[tauri::command]
pub async fn open_repo(app: tauri::AppHandle, path: String) -> Result<Repo, AppError> {
    let recent_file = recent_repos_file(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let repo = scan_repo(Path::new(&path))?;
        let _ = recent_file.and_then(|file| add_recent_repo(&file, &path));
        Ok(repo)
    })
    .await
    .map_err(|e| AppError::Platform("scanning did not finish".into(), e.into()))?
}

#[tauri::command]
pub fn list_recent_repos(app: tauri::AppHandle) -> Result<Vec<String>, AppError> {
    read_recent_repos(&recent_repos_file(&app)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Repo {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/scan")
            .join(name);
        scan_repo(&root).unwrap()
    }

    fn project(id: &str, name: &str, language: Language, root: &str, files: &[&str]) -> Project {
        Project {
            id: id.to_string(),
            name: name.to_string(),
            language,
            role: Role::Unknown,
            root: root.to_string(),
            files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    fn role_of(file_name: &str, text: &str) -> Role {
        let dir = tempfile::Builder::new().prefix("role-").tempdir().unwrap();
        fs::write(dir.path().join(file_name), text).unwrap();
        let ext = if file_name.ends_with(".csproj") {
            "Program.cs"
        } else if file_name.ends_with(".json") {
            "a.ts"
        } else {
            "a.py"
        };
        fs::write(dir.path().join(ext), "x").unwrap();
        scan_repo(dir.path()).unwrap().projects[0].role
    }

    #[test]
    fn role_comes_from_manifest_content() {
        let react = r#"{ "devDependencies": { "react": "19" } }"#;
        assert_eq!(role_of("package.json", react), Role::Frontend);
        assert_eq!(
            role_of("package.json", r#"{ "dependencies": { "express": "5" } }"#),
            Role::Backend
        );
        assert_eq!(
            role_of("pyproject.toml", "dependencies = [\"FastAPI>=0.1\"]"),
            Role::Backend
        );
        assert_eq!(role_of("requirements.txt", "Flask==3\n"), Role::Backend);
        assert_eq!(
            role_of(
                "A.csproj",
                r#"<Project Sdk="Microsoft.NET.Sdk.Web"></Project>"#
            ),
            Role::Backend
        );
        assert_eq!(
            role_of(
                "A.csproj",
                r#"<Project Sdk="Microsoft.NET.Sdk.BlazorWebAssembly"></Project>"#
            ),
            Role::Frontend
        );
    }

    #[test]
    fn role_is_unknown_without_a_clear_signal() {
        assert_eq!(
            role_of(
                "package.json",
                r#"{ "dependencies": { "react": "19", "express": "5" } }"#
            ),
            Role::Unknown
        );
        assert_eq!(
            role_of("package.json", r#"{ "dependencies": { "next": "15" } }"#),
            Role::Unknown
        );
        assert_eq!(role_of("package.json", "not json"), Role::Unknown);
        assert_eq!(
            role_of("pyproject.toml", "[project]\nname = \"cli\"\n"),
            Role::Unknown
        );
        assert_eq!(
            role_of("A.csproj", r#"<Project Sdk="Microsoft.NET.Sdk"></Project>"#),
            Role::Unknown
        );
    }

    #[test]
    fn root_projects_of_two_languages_get_their_own_roles() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{ "dependencies": { "vue": "3" } }"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("pyproject.toml"),
            "dependencies = [\"django\"]",
        )
        .unwrap();
        write_files(dir.path(), &["web.ts", "app.py"]);

        let roles: Vec<(Language, Role)> = scan_repo(dir.path())
            .unwrap()
            .projects
            .iter()
            .map(|p| (p.language, p.role))
            .collect();

        assert_eq!(
            roles,
            vec![
                (Language::Typescript, Role::Frontend),
                (Language::Python, Role::Backend)
            ]
        );
    }

    fn write_files(root: &Path, files: &[&str]) {
        for file in files {
            let path = root.join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "x").unwrap();
        }
    }

    #[test]
    fn python_fixture_has_one_project_per_manifest_folder() {
        let repo = fixture("python");
        assert_eq!(repo.name, "python");
        assert_eq!(
            repo.projects,
            vec![
                project(
                    "pyproject.toml",
                    "python",
                    Language::Python,
                    "",
                    &["app/__init__.py", "app/main.py", "tests/test_main.py"],
                ),
                project(
                    "tools/setup.py",
                    "tools",
                    Language::Python,
                    "tools",
                    &["tools/run.py", "tools/setup.py"]
                ),
            ]
        );
    }

    #[test]
    fn dotnet_fixture_has_one_project_per_csproj() {
        assert_eq!(
            fixture("dotnet").projects,
            vec![
                project(
                    "src/Api/Api.csproj",
                    "Api",
                    Language::Dotnet,
                    "src/Api",
                    &["src/Api/Models/User.cs", "src/Api/Program.cs"],
                ),
                project(
                    "src/Core/Core.csproj",
                    "Core",
                    Language::Dotnet,
                    "src/Core",
                    &["src/Core/Service.cs"]
                ),
            ]
        );
    }

    #[test]
    fn ts_workspace_root_without_own_sources_is_not_a_project() {
        assert_eq!(
            fixture("ts-workspaces").projects,
            vec![
                project(
                    "packages/api/package.json",
                    "api",
                    Language::Typescript,
                    "packages/api",
                    &["packages/api/index.js"],
                ),
                project(
                    "packages/ui/package.json",
                    "ui",
                    Language::Typescript,
                    "packages/ui",
                    &["packages/ui/src/Button.tsx", "packages/ui/src/index.ts"],
                ),
            ]
        );
    }

    #[test]
    fn mixed_fixture_uses_nearest_manifest_of_the_same_language() {
        assert_eq!(
            fixture("mixed").projects,
            vec![
                project(
                    "pyproject.toml",
                    "mixed",
                    Language::Python,
                    "",
                    &[
                        "lib/util.py",
                        "scripts/tool.py",
                        "service/scripts/helper.py",
                        "web/tools/gen.py"
                    ],
                ),
                project(
                    "service/Service.csproj",
                    "Service",
                    Language::Dotnet,
                    "service",
                    &["service/Program.cs"]
                ),
                project(
                    "web/package.json",
                    "web",
                    Language::Typescript,
                    "web",
                    &["web/src/app.ts", "web/src/legacy/old.js"],
                ),
                project(
                    "web/server/package.json",
                    "server",
                    Language::Typescript,
                    "web/server",
                    &["web/server/index.js"],
                ),
                project(
                    OTHER_PROJECT_ID,
                    OTHER_PROJECT_NAME,
                    Language::Other,
                    "",
                    &["loose.ts"],
                ),
            ]
        );
    }

    #[test]
    fn walk_skips_build_dirs_and_gitignored_files_without_git() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &[
                "kept.py",
                "ignored.py",
                "bin/a.cs",
                "obj/b.cs",
                "node_modules/dep/c.js",
                "dist/d.js",
                ".venv/e.py",
                ".ainspector/f.py",
                "target/g.py",
                "__pycache__/h.py",
                ".hidden/i.py",
                "sub/bin/j.cs",
            ],
        );
        fs::write(dir.path().join(".gitignore"), "ignored.py\n").unwrap();

        let repo = scan_repo(dir.path()).unwrap();

        assert_eq!(
            repo.projects,
            vec![project(
                OTHER_PROJECT_ID,
                OTHER_PROJECT_NAME,
                Language::Other,
                "",
                &["kept.py"]
            )]
        );
    }

    #[test]
    fn walk_ignores_gitignore_files_above_the_repo() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "ignored.py\n").unwrap();
        let root = dir.path().join("repo");
        write_files(&root, &["kept.py", "ignored.py"]);

        let repo = scan_repo(&root).unwrap();

        assert_eq!(repo.projects[0].files, vec!["ignored.py", "kept.py"]);
    }

    #[test]
    fn pyproject_wins_over_setup_py_in_one_folder() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(dir.path(), &["setup.py", "pyproject.toml", "m.py"]);

        let projects = scan_repo(dir.path()).unwrap().projects;

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].id, "pyproject.toml");
        assert_eq!(projects[0].files, vec!["m.py", "setup.py"]);
    }

    #[test]
    fn project_without_source_files_is_not_listed() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(dir.path(), &["package.json", "App.sln", "README.md"]);

        assert!(scan_repo(dir.path()).unwrap().projects.is_empty());
    }

    #[test]
    fn scan_rejects_a_path_that_is_not_a_directory() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        let error = scan_repo(&dir.path().join("missing"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("is not a directory"), "{error}");
    }

    #[test]
    fn recent_repos_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            read_recent_repos(&dir.path().join(RECENT_REPOS_FILE)).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn recent_repos_moves_a_repeated_repo_to_the_front() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("app-data").join(RECENT_REPOS_FILE);
        for repo in ["a", "b", "c", "a"] {
            add_recent_repo(&file, repo).unwrap();
        }

        assert_eq!(read_recent_repos(&file).unwrap(), vec!["a", "c", "b"]);
    }

    #[test]
    fn recent_repos_keeps_the_ten_newest() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(RECENT_REPOS_FILE);
        for n in 0..12 {
            add_recent_repo(&file, &format!("repo{n}")).unwrap();
        }

        let recent = read_recent_repos(&file).unwrap();
        assert_eq!(recent.len(), MAX_RECENT_REPOS);
        assert_eq!(recent.first().map(String::as_str), Some("repo11"));
        assert_eq!(recent.last().map(String::as_str), Some("repo2"));
    }

    #[test]
    fn recent_repos_corrupt_file_is_empty_and_replaced_by_the_next_repo() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(RECENT_REPOS_FILE);
        fs::write(&file, "not json").unwrap();

        assert_eq!(read_recent_repos(&file).unwrap(), Vec::<String>::new());
        add_recent_repo(&file, "a").unwrap();
        assert_eq!(read_recent_repos(&file).unwrap(), vec!["a"]);
    }
}
