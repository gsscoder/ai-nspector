use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use ignore::{DirEntry, WalkBuilder};
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
/// Hidden folders at the repo root that the doc walk enters; the project walk never does.
const HIDDEN_DOC_DIRS: [&str; 3] = [".github", ".agent-files", ".cursor"];
/// Hidden folders whose files are all suggested.
const AGENT_DIRS: [&str; 2] = [".agent-files", ".cursor"];
const DOC_EXTENSIONS: [&str; 5] = ["md", "mdx", "markdown", "rst", "adoc"];
/// A `.txt` file is a doc only with one of these names, so `requirements.txt` is not.
const TEXT_DOC_NAMES: [&str; 10] = [
    "README",
    "NOTES",
    "ABOUT",
    "OVERVIEW",
    "ARCHITECTURE",
    "DESIGN",
    "INTENT",
    "SPEC",
    "CHANGELOG",
    "LICENSE",
];
/// Suggested at the repo root and at a project root, besides `README*`.
const SUGGESTED_NAMES: [&str; 7] = [
    "ARCHITECTURE",
    "DESIGN",
    "OVERVIEW",
    "INTENT",
    "SPEC",
    "CLAUDE",
    "AGENTS",
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

/// A documentation file the user can pick for the repo overview.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocFile {
    /// Relative to the repo root, forward slashes.
    pub path: String,
    pub size: u64,
    /// Pre-checked in the overview dialog.
    pub suggested: bool,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub root: String,
    pub name: String,
    pub projects: Vec<Project>,
    /// Suggested first, then by folder depth, then by path.
    pub docs: Vec<DocFile>,
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
    docs: Vec<DocFile>,
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
    let mut walked = walk_repo(root, &name);
    walked.docs.extend(walk_hidden_docs(root));
    let projects = group_into_projects(walked.manifests, walked.sources);
    rank_docs(&mut walked.docs, &projects);
    Ok(Repo {
        root: root.to_string_lossy().into_owned(),
        projects,
        docs: walked.docs,
        name,
    })
}

fn is_skipped_dir(entry: &DirEntry) -> bool {
    entry.file_type().is_some_and(|t| t.is_dir())
        && SKIPPED_DIRS.iter().any(|d| entry.file_name() == *d)
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    Some(
        relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn walk_repo(root: &Path, repo_name: &str) -> Walked {
    let walker = WalkBuilder::new(root)
        .require_git(false)
        .parents(false)
        .filter_entry(|entry| entry.depth() == 0 || !is_skipped_dir(entry))
        .build();

    let mut walked = Walked {
        manifests: Vec::new(),
        sources: Vec::new(),
        docs: Vec::new(),
    };
    for entry in walker {
        let Ok(entry) = entry else {
            continue;
        };
        if let Some(doc) = doc_file(root, &entry) {
            walked.docs.push(doc);
            continue;
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Some(path) = relative_path(root, entry.path()) else {
            continue;
        };
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

/// Docs inside the hidden folders of `HIDDEN_DOC_DIRS`. A second walk keeps the project walk
/// unchanged; it starts at the repo root, so the root `.gitignore` applies.
fn walk_hidden_docs(root: &Path) -> Vec<DocFile> {
    WalkBuilder::new(root)
        .require_git(false)
        .parents(false)
        .hidden(false)
        .filter_entry(|entry| match entry.depth() {
            0 => true,
            1 => {
                entry.file_type().is_some_and(|t| t.is_dir())
                    && HIDDEN_DOC_DIRS.iter().any(|d| entry.file_name() == *d)
            }
            _ => !entry.file_name().to_string_lossy().starts_with('.') && !is_skipped_dir(entry),
        })
        .build()
        .filter_map(Result::ok)
        .filter_map(|entry| doc_file(root, &entry))
        .collect()
}

fn doc_file(root: &Path, entry: &DirEntry) -> Option<DocFile> {
    if !entry.file_type().is_some_and(|t| t.is_file())
        || !is_doc(&entry.file_name().to_string_lossy())
    {
        return None;
    }
    Some(DocFile {
        path: relative_path(root, entry.path())?,
        size: entry.metadata().ok()?.len(),
        suggested: false,
    })
}

fn is_doc(file_name: &str) -> bool {
    let Some((stem, extension)) = file_name.rsplit_once('.') else {
        return false;
    };
    DOC_EXTENSIONS
        .iter()
        .any(|e| extension.eq_ignore_ascii_case(e))
        || (extension.eq_ignore_ascii_case("txt")
            && TEXT_DOC_NAMES.iter().any(|n| stem.eq_ignore_ascii_case(n)))
}

/// `project_roots` are the folders of the detected projects; the repo root always counts.
fn is_suggested(path: &str, project_roots: &[&str]) -> bool {
    if let Some((top, _)) = path.split_once('/')
        && AGENT_DIRS.contains(&top)
    {
        return true;
    }
    let dir = parent_dir(path);
    if dir == "docs" {
        return true;
    }
    let file_name = path.rsplit_once('/').map_or(path, |(_, name)| name);
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let is_readme = stem
        .as_bytes()
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"README"));
    (dir.is_empty() || project_roots.contains(&dir))
        && (is_readme || SUGGESTED_NAMES.iter().any(|n| stem.eq_ignore_ascii_case(n)))
}

fn rank_docs(docs: &mut [DocFile], projects: &[Project]) {
    let roots: Vec<&str> = projects.iter().map(|p| p.root.as_str()).collect();
    for doc in docs.iter_mut() {
        doc.suggested = is_suggested(&doc.path, &roots);
    }
    let depth = |path: &str| path.matches('/').count();
    docs.sort_by(|a, b| {
        b.suggested
            .cmp(&a.suggested)
            .then_with(|| depth(&a.path).cmp(&depth(&b.path)))
            .then_with(|| a.path.cmp(&b.path))
    });
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

/// Index in the manifest list of the manifest that owns `source`: the nearest of the same
/// language; without one, the nearest of any language (for example the JS files of a Python web app).
fn nearest_manifest(
    by_location: &BTreeMap<(&str, Language), (usize, &Manifest)>,
    source: &SourceFile,
) -> Option<usize> {
    let dirs = || {
        std::iter::successors(Some(parent_dir(&source.path)), |dir| {
            (!dir.is_empty()).then(|| parent_dir(dir))
        })
    };
    dirs()
        .find_map(|dir| by_location.get(&(dir, source.language)))
        .or_else(|| {
            dirs().find_map(|dir| {
                by_location
                    .range((dir, Language::Python)..=(dir, Language::Other))
                    .next()
                    .map(|(_, found)| found)
            })
        })
        .map(|(index, _)| *index)
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
    fn mixed_fixture_uses_nearest_manifest_of_the_same_language_else_nearest_of_any() {
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
                        "loose.ts",
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

    fn docs_of(root: &Path) -> Vec<(String, bool)> {
        scan_repo(root)
            .unwrap()
            .docs
            .into_iter()
            .map(|d| (d.path, d.suggested))
            .collect()
    }

    fn doc(path: &str, suggested: bool) -> DocFile {
        DocFile {
            path: path.to_string(),
            size: 1,
            suggested,
        }
    }

    #[test]
    fn doc_extensions_decide_which_files_are_docs() {
        for name in ["a.md", "a.MDX", "a.markdown", "a.rst", "a.adoc"] {
            assert!(is_doc(name), "{name}");
        }
        for name in ["a.py", "a.html", "README", "md", "notes.md.bak"] {
            assert!(!is_doc(name), "{name}");
        }
    }

    #[test]
    fn txt_is_a_doc_only_with_a_doc_like_name() {
        for name in ["NOTES.txt", "notes.txt", "README.txt", "LICENSE.txt"] {
            assert!(is_doc(name), "{name}");
        }
        for name in ["requirements.txt", "CMakeLists.txt", "robots.txt"] {
            assert!(!is_doc(name), "{name}");
        }
    }

    #[test]
    fn docs_list_the_allowed_hidden_folders_only() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &[
                ".github/CONTRIBUTING.md",
                ".github/ISSUE_TEMPLATE/bug.md",
                ".github/.secret.md",
                ".github/workflows/ci.py",
                ".agent-files/spec.md",
                ".cursor/rules/style.md",
                ".hidden/a.md",
                ".ainspector/overview.md",
                ".venv/lib/a.md",
                ".github/node_modules/a.md",
                "node_modules/dep/a.md",
                "bin/a.md",
                "ignored.md",
                ".cursor-old/a.md",
            ],
        );
        fs::write(dir.path().join(".gitignore"), "ignored.md\n").unwrap();

        let paths: Vec<String> = docs_of(dir.path())
            .into_iter()
            .map(|(path, _)| path)
            .collect();

        assert_eq!(
            paths,
            vec![
                ".agent-files/spec.md",
                ".cursor/rules/style.md",
                ".github/CONTRIBUTING.md",
                ".github/ISSUE_TEMPLATE/bug.md",
            ]
        );
    }

    #[test]
    fn the_root_gitignore_applies_to_hidden_doc_folders() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(dir.path(), &[".cursor/a.md", ".github/a.md"]);
        fs::write(dir.path().join(".gitignore"), ".cursor/\n").unwrap();

        assert_eq!(
            docs_of(dir.path()),
            vec![(".github/a.md".to_string(), false)]
        );
    }

    #[test]
    fn doc_files_do_not_change_the_projects() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &[
                "a.py",
                ".github/scripts/b.py",
                ".github/package.json",
                ".cursor/c.ts",
            ],
        );

        let repo = scan_repo(dir.path()).unwrap();

        assert_eq!(repo.projects.len(), 1);
        assert_eq!(repo.projects[0].files, vec!["a.py"]);
    }

    #[test]
    fn suggested_docs_are_the_overview_sources() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &[
                "web/package.json",
                "web/a.ts",
                "README.md",
                "ARCHITECTURE.md",
                "CLAUDE.md",
                "AGENTS.md",
                "docs/guide.md",
                "web/README.md",
                "web/DESIGN.md",
                ".agent-files/intent.md",
                ".cursor/rules/style.md",
                "CHANGELOG.md",
                "LICENSE.txt",
                "CODE_OF_CONDUCT.md",
                "SECURITY.md",
                "docs/api/routes.md",
                "web/docs/DESIGN.md",
                "other/README.md",
                "other/deep/README.md",
                "web/CHANGELOG.md",
                ".github/PULL_REQUEST_TEMPLATE.md",
                ".github/ISSUE_TEMPLATE/bug.md",
            ],
        );

        let (suggested, others): (Vec<_>, Vec<_>) = docs_of(dir.path())
            .into_iter()
            .partition(|(_, suggested)| *suggested);
        let paths =
            |docs: Vec<(String, bool)>| docs.into_iter().map(|(path, _)| path).collect::<Vec<_>>();

        assert_eq!(
            paths(suggested),
            vec![
                "AGENTS.md",
                "ARCHITECTURE.md",
                "CLAUDE.md",
                "README.md",
                ".agent-files/intent.md",
                "docs/guide.md",
                "web/DESIGN.md",
                "web/README.md",
                ".cursor/rules/style.md",
            ]
        );
        assert_eq!(
            paths(others),
            vec![
                "CHANGELOG.md",
                "CODE_OF_CONDUCT.md",
                "LICENSE.txt",
                "SECURITY.md",
                ".github/PULL_REQUEST_TEMPLATE.md",
                "other/README.md",
                "web/CHANGELOG.md",
                ".github/ISSUE_TEMPLATE/bug.md",
                "docs/api/routes.md",
                "other/deep/README.md",
                "web/docs/DESIGN.md",
            ]
        );
    }

    #[test]
    fn docs_are_sorted_by_suggested_then_depth_then_path() {
        let mut docs = vec![
            doc("z/deep/x.md", false),
            doc("docs/b.md", false),
            doc("a/x.md", false),
            doc("CHANGELOG.md", false),
            doc("docs/a.md", false),
            doc("README.md", false),
            doc("docs/api/x.md", false),
        ];

        rank_docs(&mut docs, &[]);

        let paths: Vec<&str> = docs.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "README.md",
                "docs/a.md",
                "docs/b.md",
                "CHANGELOG.md",
                "a/x.md",
                "docs/api/x.md",
                "z/deep/x.md",
            ]
        );
        assert_eq!(docs.iter().filter(|d| d.suggested).count(), 3);
    }

    #[test]
    fn doc_size_is_the_file_size_in_bytes() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        fs::write(dir.path().join("README.md"), "12345").unwrap();

        assert_eq!(
            scan_repo(dir.path()).unwrap().docs,
            vec![DocFile {
                path: "README.md".to_string(),
                size: 5,
                suggested: true,
            }]
        );
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
