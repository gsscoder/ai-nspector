use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use ignore::{DirEntry, WalkBuilder};
use serde::Serialize;
use tauri::Manager;
use toml::{Table, Value as TomlValue};

use crate::error::AppError;
use crate::parse::Dialect;

/// Skipped by name. `bin`, `obj` and `target` are build output only next to a manifest of their
/// tool chain; see `is_build_output_dir`.
const SKIPPED_DIRS: [&str; 5] = [
    "node_modules",
    ".venv",
    "dist",
    ".ainspector",
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
/// Language, dependency name, display name, kind. The kind is `Frontend`, `Backend`, `Desktop`,
/// `Fullstack` or `Cli`. Python names are normalised (lowercase, `-`); for .NET the "dependency" is
/// a text marker of the project file.
type Framework = (Language, &'static str, &'static str, Role);
// One line per entry reads as a table; rustfmt would split the long ones over six lines.
#[rustfmt::skip]
const FRAMEWORKS: &[Framework] = &[
    (Language::Typescript, "react", "React", Role::Frontend),
    (Language::Typescript, "vue", "Vue", Role::Frontend),
    (Language::Typescript, "svelte", "Svelte", Role::Frontend),
    (Language::Typescript, "@angular/core", "Angular", Role::Frontend),
    (Language::Typescript, "next", "Next.js", Role::Fullstack),
    (Language::Typescript, "nuxt", "Nuxt", Role::Fullstack),
    (Language::Typescript, "@sveltejs/kit", "SvelteKit", Role::Fullstack),
    (Language::Typescript, "@remix-run/react", "Remix", Role::Fullstack),
    (Language::Typescript, "express", "Express", Role::Backend),
    (Language::Typescript, "fastify", "Fastify", Role::Backend),
    (Language::Typescript, "@nestjs/core", "NestJS", Role::Backend),
    (Language::Typescript, "hono", "Hono", Role::Backend),
    (Language::Typescript, "koa", "Koa", Role::Backend),
    (Language::Typescript, "@tauri-apps/api", "Tauri", Role::Desktop),
    (Language::Typescript, "electron", "Electron", Role::Desktop),
    (Language::Typescript, "commander", "Commander", Role::Cli),
    (Language::Typescript, "yargs", "Yargs", Role::Cli),
    (Language::Python, "fastapi", "FastAPI", Role::Backend),
    (Language::Python, "flask", "Flask", Role::Backend),
    (Language::Python, "django", "Django", Role::Backend),
    (Language::Python, "click", "Click", Role::Cli),
    (Language::Python, "typer", "Typer", Role::Cli),
    (Language::Rust, "leptos", "Leptos", Role::Frontend),
    (Language::Rust, "yew", "Yew", Role::Frontend),
    (Language::Rust, "dioxus", "Dioxus", Role::Frontend),
    (Language::Rust, "sycamore", "Sycamore", Role::Frontend),
    (Language::Rust, "axum", "Axum", Role::Backend),
    (Language::Rust, "actix-web", "Actix Web", Role::Backend),
    (Language::Rust, "rocket", "Rocket", Role::Backend),
    (Language::Rust, "warp", "Warp", Role::Backend),
    (Language::Rust, "poem", "Poem", Role::Backend),
    (Language::Rust, "tonic", "Tonic", Role::Backend),
    (Language::Rust, "tauri", "Tauri", Role::Desktop),
    (Language::Rust, "clap", "Clap", Role::Cli),
    (Language::Dotnet, "Microsoft.NET.Sdk.BlazorWebAssembly\"", "Blazor WebAssembly", Role::Frontend),
    (Language::Dotnet, "Microsoft.NET.Sdk.Web\"", "ASP.NET Core", Role::Backend),
    (Language::Dotnet, "Microsoft.NET.Sdk.Worker\"", "Worker Service", Role::Backend),
    (Language::Dotnet, "<UseWPF>true", "WPF", Role::Desktop),
    (Language::Dotnet, "<UseWindowsForms>true", "Windows Forms", Role::Desktop),
    (Language::Dotnet, "<UseMaui>true", ".NET MAUI", Role::Desktop),
];
/// A manifest folder with one of these path segments (any case) holds sample code, not a product.
const FIXTURE_FOLDERS: [&str; 5] = [
    "fixtures",
    "__fixtures__",
    "testdata",
    "examples",
    "samples",
];
const TEST_FOLDERS: [&str; 3] = ["test", "tests", "e2e"];
const TEST_FOLDER_SUFFIXES: [&str; 6] = [".tests", ".test", "-tests", "-test", "_tests", "_test"];
const DOTNET_TEST_MARKERS: [&str; 3] = [
    "Microsoft.NET.Test.Sdk",
    "MSTest.Sdk",
    "<IsTestProject>true",
];
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
    Rust,
    Other,
}

/// Read from the manifest content; `Fixture` and `Test` also from the folder name; `Unknown` when
/// the signals are absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Backend,
    Frontend,
    Desktop,
    /// A framework that serves page and API, or both frontend and backend dependencies.
    Fullstack,
    Library,
    Cli,
    Fixture,
    Test,
    Unknown,
}

/// All paths are relative to the repo root and use forward slashes.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub language: Language,
    /// `language` first, then every other language with at least a fifth of the files, most files first.
    pub languages: Vec<Language>,
    pub role: Role,
    /// Display names of the known frameworks in the manifest, sorted and unique.
    pub frameworks: Vec<String>,
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
    detection: Detection,
    /// Lower wins when one folder holds several manifests of one language.
    priority: u8,
}

impl Manifest {
    fn new(
        path: String,
        file_name: &str,
        language: Language,
        priority: u8,
        repo_name: &str,
        text: &str,
    ) -> Self {
        let dir = manifest_dir(&path, file_name).to_string();
        let detection = detect_stack(file_name, &dir, language, text);
        let name = declared_name(file_name, text).unwrap_or_else(|| {
            match file_name.strip_suffix(".csproj") {
                Some(stem) => stem.to_string(),
                None if dir.is_empty() => repo_name.to_string(),
                None => dir.rsplit('/').next().unwrap_or(&dir).to_string(),
            }
        });
        Manifest {
            id: path,
            name,
            dir,
            language,
            detection,
            priority,
        }
    }

    fn into_project(self, sources: Vec<SourceFile>) -> Project {
        Project {
            id: self.id,
            name: self.name,
            language: self.language,
            languages: project_languages(self.language, &sources),
            role: self.detection.role,
            frameworks: self.detection.frameworks,
            root: self.dir,
            files: sources.into_iter().map(|source| source.path).collect(),
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
        && (SKIPPED_DIRS.iter().any(|d| entry.file_name() == *d) || is_build_output_dir(entry))
}

// ponytail: only a `.csproj` (not `.fsproj` or `.vbproj`) marks `bin` and `obj` as .NET output; a
// build output folder next to no manifest is scanned unless `.gitignore` lists it.
fn is_build_output_dir(entry: &DirEntry) -> bool {
    let name = entry.file_name();
    let manifest_matches: fn(&str) -> bool = if name == "bin" || name == "obj" {
        |file_name| file_name.ends_with(".csproj")
    } else if name == "target" {
        |file_name| file_name == "Cargo.toml"
    } else {
        return false;
    };
    entry
        .path()
        .parent()
        .and_then(|parent| fs::read_dir(parent).ok())
        .is_some_and(|mut siblings| {
            siblings.any(|sibling| {
                sibling.is_ok_and(|s| manifest_matches(&s.file_name().to_string_lossy()))
            })
        })
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
        match (
            manifest_kind(&file_name, parent_dir(&path)),
            source_language(&file_name),
        ) {
            (Some((language, priority)), source) => {
                // `setup.py` is both a manifest and a source file.
                let source_file = source.map(|language| SourceFile {
                    path: path.clone(),
                    language,
                });
                let text = read_manifest(entry.path());
                walked.manifests.push(Manifest::new(
                    path, &file_name, language, priority, repo_name, &text,
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

// ponytail: a `tsconfig.json` alone is not a manifest: it configures a project, it does not declare one.
// A `requirements` file is any `requirements-*.txt` or `*.txt` in a folder named `requirements`;
// `constraints.txt` and other names are not read. `requirements.txt` outranks them: they are
// variants of it. A `pyproject.toml` without a dependency list (dynamic dependencies) still beats a
// `requirements.txt` with one, so the project gets no role; only one manifest per folder is read.
// A prose `*.txt` note in a `requirements` folder is not decidable by name and counts as a manifest.
fn manifest_kind(file_name: &str, dir: &str) -> Option<(Language, u8)> {
    match file_name {
        "pyproject.toml" => Some((Language::Python, 0)),
        "setup.py" => Some((Language::Python, 1)),
        "setup.cfg" | "Pipfile" => Some((Language::Python, 2)),
        "requirements.txt" => Some((Language::Python, 3)),
        "package.json" => Some((Language::Typescript, 0)),
        "deno.json" | "deno.jsonc" => Some((Language::Typescript, 1)),
        "Cargo.toml" => Some((Language::Rust, 0)),
        _ if file_name.ends_with(".csproj") => Some((Language::Dotnet, 0)),
        _ if is_requirements_file(file_name, dir) => Some((Language::Python, 4)),
        _ => None,
    }
}

fn is_requirements_file(file_name: &str, dir: &str) -> bool {
    file_name.ends_with(".txt")
        && (file_name.starts_with("requirements-")
            || dir.rsplit('/').next() == Some("requirements"))
}

/// A `requirements/*.txt` file belongs to the folder that holds `requirements/`, so it owns the
/// sources there; any other manifest belongs to its own folder.
fn manifest_dir<'a>(path: &'a str, file_name: &str) -> &'a str {
    let dir = parent_dir(path);
    if is_requirements_file(file_name, dir) && dir.rsplit('/').next() == Some("requirements") {
        parent_dir(dir)
    } else {
        dir
    }
}

// ponytail: a UTF-16 file (PowerShell 5 `pip freeze >`) is not valid UTF-8, so `read_to_string`
// fails and the manifest text is empty.
fn read_manifest(path: &Path) -> String {
    fs::metadata(path)
        .ok()
        .filter(|m| m.len() <= MAX_MANIFEST_BYTES)
        .and_then(|_| fs::read_to_string(path).ok())
        .unwrap_or_default()
}

// ponytail: only `package.json`, `Cargo.toml` and `pyproject.toml` declare a name; `setup.py`
// is not run or parsed, so it falls back to the folder name.
fn declared_name(file_name: &str, text: &str) -> Option<String> {
    let name = match file_name {
        "package.json" => serde_json::from_str::<serde_json::Value>(text)
            .ok()?
            .get("name")?
            .as_str()?
            .to_string(),
        "Cargo.toml" => {
            let manifest = text.parse::<Table>().ok()?;
            manifest.get("package")?.get("name")?.as_str()?.to_string()
        }
        "pyproject.toml" => {
            let manifest = text.parse::<Table>().ok()?;
            let project = manifest.get("project").and_then(|table| table.get("name"));
            let poetry = manifest
                .get("tool")
                .and_then(|tool| tool.get("poetry"))
                .and_then(|table| table.get("name"));
            [project, poetry]
                .into_iter()
                .flatten()
                .filter_map(TomlValue::as_str)
                .find(|name| !name.trim().is_empty())?
                .to_string()
        }
        _ => return None,
    };
    Some(name).filter(|name| !name.trim().is_empty())
}

/// Role and frameworks of one manifest.
struct Detection {
    role: Role,
    frameworks: Vec<String>,
}

/// What a manifest declares: parsed where a reader exists, else its text.
enum Declared<'a> {
    Npm(serde_json::Value),
    Names(Vec<String>),
    Text(&'a str),
}

impl<'a> Declared<'a> {
    // ponytail: `setup.py`, `setup.cfg` and `Pipfile` are matched by whole package-name tokens, so
    // `clickhouse-driver` is not `click`; a comment or description that holds the exact name still
    // matches. The .NET project file is matched as plain text.
    fn read(file_name: &str, language: Language, text: &'a str) -> Self {
        match language {
            Language::Typescript => Self::Npm(serde_json::from_str(text).unwrap_or_default()),
            Language::Python if file_name == "pyproject.toml" => {
                Self::Names(pyproject_dependency_names(text))
            }
            Language::Python if file_name.ends_with(".txt") => Self::Names(
                text.trim_start_matches('\u{feff}')
                    .lines()
                    .filter_map(requirement_name)
                    .collect(),
            ),
            Language::Python => Self::Names(python_token_names(text)),
            Language::Rust => Self::Names(cargo_dependency_names(text)),
            Language::Dotnet | Language::Other => Self::Text(text),
        }
    }

    fn has(&self, dependency: &str) -> bool {
        match self {
            Self::Npm(json) => ["dependencies", "devDependencies"]
                .iter()
                .any(|section| json[*section].get(dependency).is_some()),
            Self::Names(names) => names.iter().any(|name| name == dependency),
            Self::Text(text) => text.contains(dependency),
        }
    }

    /// A `package.json` with a `bin` field installs a command.
    fn has_npm_bin(&self) -> bool {
        matches!(self, Self::Npm(json) if json.get("bin").is_some())
    }
}

fn detect_stack(file_name: &str, dir: &str, language: Language, text: &str) -> Detection {
    let declared = Declared::read(file_name, language, text);
    let known: Vec<(&str, Role)> = FRAMEWORKS
        .iter()
        .filter(|(framework_language, dependency, ..)| {
            *framework_language == language && declared.has(dependency)
        })
        .map(|&(_, _, name, kind)| (name, kind))
        .collect();
    let mut kinds: Vec<Role> = known.iter().map(|&(_, kind)| kind).collect();
    if declared.has_npm_bin() {
        kinds.push(Role::Cli);
    }
    Detection {
        role: detect_role(file_name, dir, text, &kinds),
        frameworks: known
            .iter()
            .map(|&(name, _)| name)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

// ponytail: plain name match, no version or script check. `Desktop` outranks the rest; frontend
// and backend together, or a fullstack framework, give `Fullstack`; `Cli` is the weakest signal.
fn detect_role(file_name: &str, dir: &str, text: &str, kinds: &[Role]) -> Role {
    if is_fixture_project(dir) {
        return Role::Fixture;
    }
    if is_test_project(file_name, dir, text) {
        return Role::Test;
    }
    if is_library_project(file_name, text) {
        return Role::Library;
    }
    let has = |kind| kinds.contains(&kind);
    if has(Role::Desktop) {
        Role::Desktop
    } else if has(Role::Fullstack) || (has(Role::Frontend) && has(Role::Backend)) {
        Role::Fullstack
    } else if has(Role::Frontend) {
        Role::Frontend
    } else if has(Role::Backend) {
        Role::Backend
    } else if has(Role::Cli) {
        Role::Cli
    } else {
        Role::Unknown
    }
}

/// Runtime dependencies of `[dependencies]`, `[workspace.dependencies]` and
/// `[target.*.dependencies]` by package name, so a renamed dependency counts under its real name.
// ponytail: dev and build dependencies are skipped on purpose; an invalid manifest has no names.
fn cargo_dependency_names(text: &str) -> Vec<String> {
    let Ok(manifest) = text.parse::<Table>() else {
        return Vec::new();
    };
    let workspace = manifest
        .get("workspace")
        .and_then(|table| table.get("dependencies"));
    let targets = manifest
        .get("target")
        .and_then(TomlValue::as_table)
        .into_iter()
        .flat_map(Table::values)
        .filter_map(|target| target.get("dependencies"));
    manifest
        .get("dependencies")
        .into_iter()
        .chain(workspace)
        .chain(targets)
        .filter_map(TomlValue::as_table)
        .flat_map(Table::iter)
        .map(|(key, spec)| {
            spec.get("package")
                .and_then(TomlValue::as_str)
                .unwrap_or(key)
                .to_string()
        })
        .collect()
}

/// Names from `[project] dependencies`, `[project.optional-dependencies]` and
/// `[tool.poetry.dependencies]`, normalised like `requirement_name`.
// ponytail: dependency groups (PEP 735, Poetry `group.*`) and Poetry extras are not read; an
// invalid file has no names.
fn pyproject_dependency_names(text: &str) -> Vec<String> {
    let Ok(pyproject) = text.parse::<Table>() else {
        return Vec::new();
    };
    let project = pyproject.get("project");
    let optional = project
        .and_then(|table| table.get("optional-dependencies"))
        .and_then(TomlValue::as_table)
        .into_iter()
        .flat_map(Table::values);
    let requirements = project
        .and_then(|table| table.get("dependencies"))
        .into_iter()
        .chain(optional)
        .filter_map(TomlValue::as_array)
        .flatten()
        .filter_map(TomlValue::as_str)
        .filter_map(requirement_name);
    let poetry = pyproject
        .get("tool")
        .and_then(|tool| tool.get("poetry"))
        .and_then(|table| table.get("dependencies"))
        .and_then(TomlValue::as_table)
        .into_iter()
        .flat_map(Table::keys)
        .map(|name| normalize_python_name(name));
    requirements.chain(poetry).collect()
}

/// The package name of a PEP 508 requirement or a `requirements.txt` line; `None` for a comment,
/// an option (`-r other.txt`), a URL or a path.
fn requirement_name(requirement: &str) -> Option<String> {
    let requirement = requirement.trim();
    let end = requirement
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(requirement.len());
    let (name, rest) = requirement.split_at(end);
    if !name.starts_with(|c: char| c.is_ascii_alphanumeric()) {
        return None;
    }
    rest.chars()
        .next()
        .is_none_or(|c| c.is_whitespace() || "[<>=!~;(@".contains(c))
        .then(|| normalize_python_name(name))
}

/// Every run of name characters in `text`, normalised like `requirement_name`; a line with `::`
/// is a trove classifier (`Framework :: Django`), not a dependency.
fn python_token_names(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| !line.contains("::"))
        .flat_map(|line| {
            line.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        })
        .filter(|token| !token.is_empty())
        .map(normalize_python_name)
        .collect()
}

/// PEP 503: lowercase, and each run of `-`, `_` and `.` becomes one `-`.
fn normalize_python_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '-' | '_' | '.') {
            if !normalized.ends_with('-') {
                normalized.push('-');
            }
        } else {
            normalized.push(c.to_ascii_lowercase());
        }
    }
    normalized
}

fn is_fixture_project(dir: &str) -> bool {
    dir.split('/').any(|segment| {
        FIXTURE_FOLDERS
            .iter()
            .any(|folder| segment.eq_ignore_ascii_case(folder))
    })
}

// ponytail: only a .NET project file with an explicit `<OutputType>Library` is a library; for
// Cargo, Python and npm the signal is in the file list (`src/lib.rs` without `src/main.rs`), not
// in the manifest, so they never get `Library`.
fn is_library_project(file_name: &str, text: &str) -> bool {
    file_name.ends_with(".csproj") && text.contains("<OutputType>Library")
}

// ponytail: only .NET has a test marker in the manifest; Python and JS/TS test projects are
// found by folder name, a test-only package elsewhere keeps its dependency role.
fn is_test_project(file_name: &str, dir: &str, text: &str) -> bool {
    let folder = dir.rsplit('/').next().unwrap_or(dir).to_lowercase();
    TEST_FOLDERS.contains(&folder.as_str())
        || TEST_FOLDER_SUFFIXES.iter().any(|suffix| folder.ends_with(suffix))
        || (file_name.ends_with(".csproj")
            && DOTNET_TEST_MARKERS.iter().any(|marker| text.contains(marker)))
}

/// Extensions with a grammar come from `Dialect`, so scan and parse cannot disagree. `.vue`,
/// `.svelte` and `.razor` have no grammar: they are listed with their stack and parse returns an
/// empty `FileInfo` for them.
fn source_language(file_name: &str) -> Option<Language> {
    match Dialect::from_path(file_name) {
        Some(Dialect::Python) => Some(Language::Python),
        Some(Dialect::CSharp) => Some(Language::Dotnet),
        Some(Dialect::TypeScript | Dialect::Tsx | Dialect::JavaScript) => {
            Some(Language::Typescript)
        }
        Some(Dialect::Rust) => Some(Language::Rust),
        None => match file_name.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
            "vue" | "svelte" => Some(Language::Typescript),
            "razor" => Some(Language::Dotnet),
            _ => None,
        },
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
    let mut sources_by_manifest: BTreeMap<usize, Vec<SourceFile>> = BTreeMap::new();
    let mut other_files = Vec::new();
    for source in sources {
        match nearest_manifest(&by_location, &source) {
            Some(index) => sources_by_manifest.entry(index).or_default().push(source),
            None => other_files.push(source.path),
        }
    }

    let mut projects: Vec<Project> = manifests
        .into_iter()
        .enumerate()
        .filter_map(|(index, manifest)| {
            sources_by_manifest
                .remove(&index)
                .map(|sources| manifest.into_project(sources))
        })
        .collect();
    projects.sort_by(|a, b| a.id.cmp(&b.id));
    if !other_files.is_empty() {
        projects.push(Project {
            id: OTHER_PROJECT_ID.to_string(),
            name: OTHER_PROJECT_NAME.to_string(),
            language: Language::Other,
            languages: vec![Language::Other],
            role: Role::Unknown,
            frameworks: Vec::new(),
            root: String::new(),
            files: other_files,
        });
    }
    projects
}

/// `primary` first, then the other languages that own at least a fifth of `sources`, most files
/// first; equal counts keep the `Language` order.
fn project_languages(primary: Language, sources: &[SourceFile]) -> Vec<Language> {
    let mut counts: BTreeMap<Language, usize> = BTreeMap::new();
    for source in sources {
        *counts.entry(source.language).or_default() += 1;
    }
    let mut others: Vec<(Language, usize)> = counts
        .into_iter()
        .filter(|&(language, count)| language != primary && count * 5 >= sources.len())
        .collect();
    others.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
    std::iter::once(primary)
        .chain(others.into_iter().map(|(language, _)| language))
        .collect()
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
            languages: vec![language],
            role: Role::Unknown,
            frameworks: Vec::new(),
            root: root.to_string(),
            files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    fn role_of(file_name: &str, text: &str) -> Role {
        scanned(file_name, text).role
    }

    fn frameworks_of(file_name: &str, text: &str) -> Vec<String> {
        scanned(file_name, text).frameworks
    }

    fn scanned(file_name: &str, text: &str) -> Project {
        let dir = tempfile::Builder::new().prefix("role-").tempdir().unwrap();
        fs::write(dir.path().join(file_name), text).unwrap();
        let ext = if file_name.ends_with(".csproj") {
            "Program.cs"
        } else if file_name.ends_with(".json") {
            "a.ts"
        } else if file_name == "Cargo.toml" {
            "lib.rs"
        } else {
            "a.py"
        };
        fs::write(dir.path().join(ext), "x").unwrap();
        scan_repo(dir.path()).unwrap().projects.remove(0)
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
            role_of(
                "pyproject.toml",
                "[project]\ndependencies = [\"FastAPI>=0.1\"]"
            ),
            Role::Backend
        );
        assert_eq!(role_of("requirements.txt", "Flask==3\n"), Role::Backend);
        assert_eq!(
            role_of("Cargo.toml", "[dependencies]\naxum = \"0.8\"\n"),
            Role::Backend
        );
        assert_eq!(
            role_of("Cargo.toml", "[dependencies]\nleptos = \"0.7\"\n"),
            Role::Frontend
        );
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
        assert_eq!(role_of("package.json", "not json"), Role::Unknown);
        assert_eq!(
            role_of("pyproject.toml", "[project]\nname = \"cli\"\n"),
            Role::Unknown
        );
        assert_eq!(
            role_of("A.csproj", r#"<Project Sdk="Microsoft.NET.Sdk"></Project>"#),
            Role::Unknown
        );
        assert_eq!(
            role_of("Cargo.toml", "[package]\nname = \"cli\"\n"),
            Role::Unknown
        );
    }

    #[test]
    fn rust_role_reads_only_runtime_dependency_names() {
        for manifest in [
            "[package]\nname = \"rocket-league-stats\"\ndescription = \"poem generator\"\n",
            "[build-dependencies]\ntonic-build = \"0.12\"\n",
            "[dev-dependencies]\naxum = \"0.8\"\n",
        ] {
            assert_eq!(role_of("Cargo.toml", manifest), Role::Unknown, "{manifest}");
        }
        for (manifest, role) in [
            ("[dependencies]\naxum.workspace = true\n", Role::Backend),
            ("[dependencies.axum]\nversion = \"0.8\"\n", Role::Backend),
            (
                "[target.'cfg(unix)'.dependencies]\nyew = \"0.21\"\n",
                Role::Frontend,
            ),
        ] {
            assert_eq!(role_of("Cargo.toml", manifest), role, "{manifest}");
        }
    }

    #[test]
    fn rust_role_reads_workspace_dependencies_and_renamed_packages() {
        for (manifest, role) in [
            ("[workspace.dependencies]\naxum = \"0.8\"\n", Role::Backend),
            (
                "[dependencies]\nweb = { package = \"axum\", version = \"0.8\" }\n",
                Role::Backend,
            ),
            // the key is only an alias: this `axum` is the `serde` package
            (
                "[dependencies]\naxum = { package = \"serde\" }\n",
                Role::Unknown,
            ),
            ("not toml [", Role::Unknown),
        ] {
            assert_eq!(role_of("Cargo.toml", manifest), role, "{manifest}");
        }
    }

    #[test]
    fn desktop_role_comes_from_a_desktop_framework() {
        let tauri_app = r#"{ "dependencies": { "react": "19", "@tauri-apps/api": "2" } }"#;
        assert_eq!(role_of("package.json", tauri_app), Role::Desktop);
        assert_eq!(
            role_of(
                "package.json",
                r#"{ "devDependencies": { "electron": "33" } }"#
            ),
            Role::Desktop
        );
        assert_eq!(
            role_of(
                "Cargo.toml",
                "[dependencies]\ntauri = \"2\"\naxum = \"0.8\"\n"
            ),
            Role::Desktop
        );
        for marker in ["UseWPF", "UseWindowsForms", "UseMaui"] {
            let project = format!(
                r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><{marker}>true</{marker}></PropertyGroup></Project>"#
            );
            assert_eq!(role_of("A.csproj", &project), Role::Desktop, "{marker}");
        }
        assert_eq!(
            frameworks_of("package.json", tauri_app),
            vec!["React", "Tauri"]
        );
    }

    #[test]
    fn fullstack_role_comes_from_a_fullstack_framework_or_both_sides() {
        for package in ["next", "nuxt", "@sveltejs/kit", "@remix-run/react"] {
            let manifest = format!(r#"{{ "dependencies": {{ "{package}": "1" }} }}"#);
            assert_eq!(
                role_of("package.json", &manifest),
                Role::Fullstack,
                "{package}"
            );
        }
        assert_eq!(
            role_of(
                "package.json",
                r#"{ "dependencies": { "react": "19", "express": "5" } }"#
            ),
            Role::Fullstack
        );
        assert_eq!(
            role_of(
                "Cargo.toml",
                "[dependencies]\nyew = \"0.21\"\naxum = \"0.8\"\n"
            ),
            Role::Fullstack
        );
    }

    #[test]
    fn cli_role_comes_from_a_command_line_framework_or_a_bin_field() {
        for (file_name, manifest) in [
            ("Cargo.toml", "[dependencies]\nclap = \"4\"\n"),
            ("requirements.txt", "click==8\n"),
            ("pyproject.toml", "[project]\ndependencies = [\"typer\"]\n"),
            (
                "package.json",
                r#"{ "dependencies": { "commander": "12" } }"#,
            ),
            ("package.json", r#"{ "dependencies": { "yargs": "17" } }"#),
            ("package.json", r#"{ "bin": { "tool": "cli.js" } }"#),
            ("package.json", r#"{ "bin": "cli.js", "dependencies": {} }"#),
        ] {
            assert_eq!(role_of(file_name, manifest), Role::Cli, "{manifest}");
        }
        assert_eq!(
            role_of(
                "package.json",
                r#"{ "bin": "cli.js", "dependencies": { "express": "5" } }"#
            ),
            Role::Backend
        );
    }

    #[test]
    fn backend_role_includes_the_dotnet_worker_sdk() {
        let worker = r#"<Project Sdk="Microsoft.NET.Sdk.Worker"></Project>"#;
        assert_eq!(role_of("A.csproj", worker), Role::Backend);
        assert_eq!(frameworks_of("A.csproj", worker), vec!["Worker Service"]);
    }

    #[test]
    fn library_role_needs_an_explicit_dotnet_output_type() {
        let library = r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup>
            <OutputType>Library</OutputType><UseWPF>true</UseWPF></PropertyGroup></Project>"#;
        assert_eq!(role_of("A.csproj", library), Role::Library);
        assert_eq!(frameworks_of("A.csproj", library), vec!["WPF"]);
        let exe = r#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup>
            <OutputType>Exe</OutputType></PropertyGroup></Project>"#;
        assert_eq!(role_of("A.csproj", exe), Role::Unknown);
        assert_eq!(
            role_of("Cargo.toml", "[package]\nname = \"lib\"\n[lib]\n"),
            Role::Unknown
        );
    }

    #[test]
    fn frameworks_are_display_names_sorted_and_unique() {
        let manifest = r#"{
            "dependencies": { "react": "19", "next": "15" },
            "devDependencies": { "react": "19", "express": "5" }
        }"#;
        assert_eq!(
            frameworks_of("package.json", manifest),
            vec!["Express", "Next.js", "React"]
        );
        assert_eq!(
            frameworks_of("Cargo.toml", "[dependencies]\nactix-web = \"4\"\n"),
            vec!["Actix Web"]
        );
        assert!(frameworks_of("package.json", r#"{ "dependencies": {} }"#).is_empty());
    }

    #[test]
    fn python_dependencies_are_read_by_package_name() {
        let text = |lines: &[&str]| lines.join("\n");
        for (file_name, manifest, expected) in [
            (
                "requirements.txt",
                text(&[
                    "# flask is not used",
                    "-r base.txt",
                    "-e .",
                    "Flask-Cors==4",
                    "django>=4 # web",
                ]),
                vec!["Django"],
            ),
            (
                "requirements.txt",
                text(&[
                    "git+https://example.com/flask.git",
                    "./flask",
                    "  Flask [async] >=2 ; python_version>'3'",
                ]),
                vec!["Flask"],
            ),
            (
                "pyproject.toml",
                text(&[
                    "[project]",
                    "description = \"a flask and django service\"",
                    "# uses fastapi",
                ]),
                vec![],
            ),
            (
                "pyproject.toml",
                text(&[
                    "[project]",
                    "dependencies = [\"FastAPI>=0.1\", \"requests\"]",
                ]),
                vec!["FastAPI"],
            ),
            (
                "pyproject.toml",
                text(&[
                    "[project.optional-dependencies]",
                    "web = [\"Flask_SQLAlchemy\", \"flask (>=2)\"]",
                ]),
                vec!["Flask"],
            ),
            (
                "pyproject.toml",
                text(&[
                    "[tool.poetry.dependencies]",
                    "python = \"^3.12\"",
                    "Django = \"^5\"",
                ]),
                vec!["Django"],
            ),
            (
                "pyproject.toml",
                text(&["[project", "dependencies = [\"flask\"]"]),
                vec![],
            ),
        ] {
            assert_eq!(frameworks_of(file_name, &manifest), expected, "{manifest}");
        }
        assert_eq!(
            role_of("pyproject.toml", "[project]\ndescription = \"flask\"\n"),
            Role::Unknown
        );
    }

    #[test]
    fn python_names_are_normalised_like_pep_503() {
        assert_eq!(
            requirement_name("Flask_SQL.Alchemy>=1").as_deref(),
            Some("flask-sql-alchemy")
        );
        assert_eq!(requirement_name("a__b--c").as_deref(), Some("a-b-c"));
        for line in [
            "",
            "# flask",
            "-r flask.txt",
            "--index-url https://x",
            "git+https://x/flask",
            "./flask",
        ] {
            assert_eq!(requirement_name(line), None, "{line}");
        }
    }

    #[test]
    fn setup_files_match_whole_package_names_only() {
        for (file_name, manifest) in [
            (
                "setup.py",
                "setup(install_requires=['clickhouse-driver', 'flask-cors>=3'])",
            ),
            ("setup.py", "setup(classifiers=['Framework :: Django'])"),
            (
                "setup.cfg",
                "[metadata]\nclassifiers =\n    Framework :: Django :: 4.2\n",
            ),
            ("Pipfile", "[packages]\nclickhouse_driver = \"*\"\n"),
        ] {
            assert_eq!(role_of(file_name, manifest), Role::Unknown, "{manifest}");
            assert!(frameworks_of(file_name, manifest).is_empty(), "{manifest}");
        }
        for (file_name, manifest, framework) in [
            ("setup.py", "setup(install_requires=['Click>=8'])", "Click"),
            (
                "setup.cfg",
                "[options]\ninstall_requires =\n    Flask_Cors\n    django>=4\n",
                "Django",
            ),
            ("Pipfile", "[packages]\nTyper = \"*\"\n", "Typer"),
        ] {
            assert_eq!(
                frameworks_of(file_name, manifest)
                    .first()
                    .map(String::as_str),
                Some(framework),
                "{manifest}"
            );
        }
    }

    #[test]
    fn requirements_file_with_a_byte_order_mark_keeps_its_first_requirement() {
        assert_eq!(
            frameworks_of("requirements.txt", "\u{feff}Flask==3\npytest\n"),
            vec!["Flask"]
        );
    }

    #[test]
    fn fixture_role_comes_from_any_folder_segment_of_the_manifest() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        let react = r#"{ "dependencies": { "react": "19" } }"#;
        for folder in [
            "examples/web",
            "tests/Fixtures/a",
            "pkg/__fixtures__",
            "TestData",
            "docs/samples/x/y",
            "examples/tests",
        ] {
            let folder = dir.path().join(folder);
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("package.json"), react).unwrap();
            fs::write(folder.join("a.ts"), "x").unwrap();
        }
        for folder in ["my-examples", "sample"] {
            let folder = dir.path().join(folder);
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("package.json"), react).unwrap();
            fs::write(folder.join("a.ts"), "x").unwrap();
        }

        let found: Vec<(String, Role, Vec<String>)> = scan_repo(dir.path())
            .unwrap()
            .projects
            .into_iter()
            .map(|p| (p.root, p.role, p.frameworks))
            .collect();

        let react_names = vec!["React".to_string()];
        assert_eq!(
            found,
            vec![
                ("TestData".to_string(), Role::Fixture, react_names.clone()),
                (
                    "docs/samples/x/y".to_string(),
                    Role::Fixture,
                    react_names.clone()
                ),
                (
                    "examples/tests".to_string(),
                    Role::Fixture,
                    react_names.clone()
                ),
                (
                    "examples/web".to_string(),
                    Role::Fixture,
                    react_names.clone()
                ),
                (
                    "my-examples".to_string(),
                    Role::Frontend,
                    react_names.clone()
                ),
                (
                    "pkg/__fixtures__".to_string(),
                    Role::Fixture,
                    react_names.clone()
                ),
                ("sample".to_string(), Role::Frontend, react_names.clone()),
                ("tests/Fixtures/a".to_string(), Role::Fixture, react_names),
            ]
        );
    }

    #[test]
    fn test_role_comes_from_folder_name_or_dotnet_test_sdk() {
        let test_sdk = r#"<Project Sdk="Microsoft.NET.Sdk.Web"><ItemGroup>
            <PackageReference Include="Microsoft.NET.Test.Sdk" Version="17" />
            </ItemGroup></Project>"#;
        assert_eq!(role_of("A.csproj", test_sdk), Role::Test);
        assert_eq!(
            role_of("A.csproj", r#"<Project Sdk="MSTest.Sdk/3.6.4"></Project>"#),
            Role::Test
        );
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &[
                "app/pyproject.toml",
                "app/m.py",
                "tests/requirements.txt",
                "tests/t.py",
                "web/e2e/package.json",
                "web/e2e/a.ts",
                "src/Api.Tests/Api.Tests.csproj",
                "src/Api.Tests/T.cs",
                "latest/package.json",
                "latest/a.ts",
            ],
        );
        let roles: Vec<(String, Role)> = scan_repo(dir.path())
            .unwrap()
            .projects
            .into_iter()
            .map(|p| (p.id, p.role))
            .collect();
        assert_eq!(
            roles,
            vec![
                ("app/pyproject.toml".to_string(), Role::Unknown),
                ("latest/package.json".to_string(), Role::Unknown),
                ("src/Api.Tests/Api.Tests.csproj".to_string(), Role::Test),
                ("tests/requirements.txt".to_string(), Role::Test),
                ("web/e2e/package.json".to_string(), Role::Test),
            ]
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
            "[project]\ndependencies = [\"django\"]",
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
                    "demo",
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
                Project {
                    // `loose.ts` is one of five files: exactly a fifth.
                    languages: vec![Language::Python, Language::Typescript],
                    ..project(
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
                    )
                },
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
    fn walk_skips_named_dirs_and_gitignored_files_without_git() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &[
                "kept.py",
                "ignored.py",
                "node_modules/dep/c.js",
                "dist/d.js",
                ".venv/e.py",
                ".ainspector/f.py",
                "__pycache__/h.py",
                ".hidden/i.py",
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

    /// Every scanned source file of a repo made of `files`, sorted.
    fn scanned_files(files: &[&str]) -> Vec<String> {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(dir.path(), files);
        let mut scanned: Vec<String> = scan_repo(dir.path())
            .unwrap()
            .projects
            .into_iter()
            .flat_map(|p| p.files)
            .collect();
        scanned.sort();
        scanned
    }

    #[test]
    fn walk_enters_bin_of_a_node_cli_and_a_python_repo() {
        assert_eq!(
            scanned_files(&["package.json", "bin/cli.js", "src/index.js"]),
            vec!["bin/cli.js", "src/index.js"]
        );
        assert_eq!(
            scanned_files(&["pyproject.toml", "bin/run.py", "app.py"]),
            vec!["app.py", "bin/run.py"]
        );
    }

    #[test]
    fn walk_skips_bin_and_obj_only_next_to_a_csproj() {
        assert_eq!(
            scanned_files(&[
                "web/Web.csproj",
                "web/Program.cs",
                "web/bin/Debug/a.cs",
                "web/obj/b.cs",
                "tools/bin/c.cs",
                "tools/obj/d.cs",
            ]),
            vec!["tools/bin/c.cs", "tools/obj/d.cs", "web/Program.cs"]
        );
    }

    #[test]
    fn walk_skips_target_only_next_to_a_cargo_toml() {
        assert_eq!(
            scanned_files(&[
                "Cargo.toml",
                "src/main.rs",
                "src/bin/tool.rs",
                "target/debug/build.rs",
                "scripts/target/gen.py",
            ]),
            vec!["scripts/target/gen.py", "src/bin/tool.rs", "src/main.rs"]
        );
    }

    #[test]
    fn source_extensions_agree_between_scan_and_parse() {
        let cases = [
            ("a.py", Some(Language::Python)),
            ("a.pyi", Some(Language::Python)),
            ("a.cs", Some(Language::Dotnet)),
            ("a.razor", Some(Language::Dotnet)),
            ("a.ts", Some(Language::Typescript)),
            ("a.mts", Some(Language::Typescript)),
            ("a.cts", Some(Language::Typescript)),
            ("a.tsx", Some(Language::Typescript)),
            ("a.js", Some(Language::Typescript)),
            ("a.jsx", Some(Language::Typescript)),
            ("a.mjs", Some(Language::Typescript)),
            ("a.cjs", Some(Language::Typescript)),
            ("a.vue", Some(Language::Typescript)),
            ("a.svelte", Some(Language::Typescript)),
            ("a.rs", Some(Language::Rust)),
            ("a.cshtml", None),
            ("a.txt", None),
            ("Makefile", None),
        ];
        for (file_name, language) in cases {
            assert_eq!(source_language(file_name), language, "{file_name}");
        }
    }

    #[test]
    fn vue_svelte_and_razor_files_are_listed_with_their_stack() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &[
                "web/package.json",
                "web/App.vue",
                "web/Page.svelte",
                "app/App.csproj",
                "app/Index.razor",
            ],
        );

        let projects = scan_repo(dir.path()).unwrap().projects;

        assert_eq!(projects[0].files, vec!["app/Index.razor"]);
        assert_eq!(projects[1].files, vec!["web/App.vue", "web/Page.svelte"]);
        let analyzed = crate::parse::analyze_project(dir.path(), &projects[1].files).unwrap();
        assert_eq!(analyzed.files.len(), 2);
        assert!(
            analyzed
                .files
                .iter()
                .all(|f| f.symbols.is_empty() && f.imports.is_empty())
        );
    }

    #[test]
    fn manifest_names_cover_python_and_deno_conventions() {
        let kind = manifest_kind;
        assert_eq!(kind("requirements.txt", ""), Some((Language::Python, 3)));
        assert_eq!(
            kind("requirements-dev.txt", ""),
            Some((Language::Python, 4))
        );
        assert_eq!(
            kind("base.txt", "requirements"),
            Some((Language::Python, 4))
        );
        assert_eq!(
            kind("base.txt", "svc/requirements"),
            Some((Language::Python, 4))
        );
        assert_eq!(kind("base.txt", "docs"), None);
        assert_eq!(kind("requirements-dev.md", ""), None);
        assert_eq!(kind("setup.cfg", ""), Some((Language::Python, 2)));
        assert_eq!(kind("Pipfile", ""), Some((Language::Python, 2)));
        assert_eq!(kind("deno.json", ""), Some((Language::Typescript, 1)));
        assert_eq!(kind("deno.jsonc", ""), Some((Language::Typescript, 1)));
        assert_eq!(kind("tsconfig.json", ""), None);
    }

    #[test]
    fn requirements_folder_file_owns_the_sources_of_the_folder_above() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &["svc/requirements/base.txt", "svc/app.py", "other/b.py"],
        );

        let projects = scan_repo(dir.path()).unwrap().projects;

        assert_eq!(projects[0].id, "svc/requirements/base.txt");
        assert_eq!(projects[0].root, "svc");
        assert_eq!(projects[0].files, vec!["svc/app.py"]);
        assert_eq!(projects[1].id, OTHER_PROJECT_ID);
        assert_eq!(projects[1].files, vec!["other/b.py"]);
    }

    #[test]
    fn manifest_priority_orders_python_and_deno_manifests_in_one_folder() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &["requirements.txt", "Pipfile", "setup.py", "m.py"],
        );
        let projects = scan_repo(dir.path()).unwrap().projects;
        assert_eq!(projects[0].id, "setup.py");

        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(dir.path(), &["requirements-dev.txt", "setup.cfg", "m.py"]);
        let projects = scan_repo(dir.path()).unwrap().projects;
        assert_eq!(projects[0].id, "setup.cfg");

        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(dir.path(), &["deno.json", "package.json", "main.ts"]);
        let projects = scan_repo(dir.path()).unwrap().projects;
        assert_eq!(projects[0].id, "package.json");
    }

    #[test]
    fn requirements_txt_wins_over_requirements_variants_in_one_folder() {
        for variants in [
            &["app/requirements-dev.txt"][..],
            &["app/requirements/base.txt", "app/requirements/prod.txt"][..],
        ] {
            let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
            write_files(dir.path(), variants);
            write_files(dir.path(), &["app/main.py"]);
            fs::write(dir.path().join("app/requirements.txt"), "flask==3\n").unwrap();

            let projects = scan_repo(dir.path()).unwrap().projects;

            assert_eq!(projects.len(), 1, "{variants:?}");
            assert_eq!(projects[0].id, "app/requirements.txt", "{variants:?}");
            assert_eq!(projects[0].role, Role::Backend, "{variants:?}");
            assert_eq!(projects[0].frameworks, vec!["Flask"], "{variants:?}");
        }
    }

    #[test]
    fn doc_named_text_files_in_a_requirements_folder_are_docs_not_manifests() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &[
                "docs/requirements/README.txt",
                "docs/requirements/LICENSE.txt",
                "docs/requirements/NOTES.txt",
                "docs/conf.py",
            ],
        );

        let repo = scan_repo(dir.path()).unwrap();

        assert_eq!(repo.projects.len(), 1);
        assert_eq!(repo.projects[0].id, OTHER_PROJECT_ID);
        assert_eq!(repo.docs.len(), 3);
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
    fn rs_files_attach_to_the_nearest_cargo_toml() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &[
                "Cargo.toml",
                "src/main.rs",
                "crates/core/Cargo.toml",
                "crates/core/src/lib.rs",
            ],
        );

        let projects = scan_repo(dir.path()).unwrap().projects;

        assert!(projects.iter().all(|p| p.language == Language::Rust));
        let found: Vec<(&str, Vec<&str>)> = projects
            .iter()
            .map(|p| (p.id.as_str(), p.files.iter().map(String::as_str).collect()))
            .collect();
        assert_eq!(
            found,
            vec![
                ("Cargo.toml", vec!["src/main.rs"]),
                ("crates/core/Cargo.toml", vec!["crates/core/src/lib.rs"]),
            ]
        );
    }

    #[test]
    fn cargo_workspace_root_without_own_sources_is_not_a_project() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(
            dir.path(),
            &["Cargo.toml", "crates/a/Cargo.toml", "crates/a/src/lib.rs"],
        );

        let ids: Vec<String> = scan_repo(dir.path())
            .unwrap()
            .projects
            .into_iter()
            .map(|p| p.id)
            .collect();

        assert_eq!(ids, vec!["crates/a/Cargo.toml"]);
    }

    fn sources_of(counts: &[(Language, usize)]) -> Vec<SourceFile> {
        counts
            .iter()
            .flat_map(|&(language, count)| {
                (0..count).map(move |i| SourceFile {
                    path: format!("{language:?}{i}"),
                    language,
                })
            })
            .collect()
    }

    #[test]
    fn languages_list_a_language_from_a_fifth_of_the_files() {
        let languages =
            |counts: &[(Language, usize)]| project_languages(Language::Python, &sources_of(counts));

        let below = languages(&[(Language::Python, 6), (Language::Typescript, 1)]);
        let at = languages(&[(Language::Python, 8), (Language::Typescript, 2)]);
        let above = languages(&[(Language::Python, 7), (Language::Typescript, 3)]);

        assert_eq!(below, vec![Language::Python]);
        assert_eq!(at, vec![Language::Python, Language::Typescript]);
        assert_eq!(above, vec![Language::Python, Language::Typescript]);
    }

    #[test]
    fn languages_put_the_primary_first_then_most_files_then_enum_order() {
        let counts = [
            (Language::Python, 3),
            (Language::Dotnet, 3),
            (Language::Typescript, 5),
            (Language::Rust, 3),
        ];

        assert_eq!(
            project_languages(Language::Python, &sources_of(&counts)),
            vec![
                Language::Python,
                Language::Typescript,
                Language::Dotnet,
                Language::Rust
            ]
        );
        assert_eq!(
            project_languages(Language::Rust, &sources_of(&counts)),
            vec![
                Language::Rust,
                Language::Typescript,
                Language::Python,
                Language::Dotnet
            ]
        );
    }

    #[test]
    fn languages_hold_the_primary_even_without_a_file_of_it() {
        let sources = sources_of(&[(Language::Typescript, 4)]);

        assert_eq!(
            project_languages(Language::Python, &sources),
            vec![Language::Python, Language::Typescript]
        );
    }

    #[test]
    fn the_other_files_project_lists_only_other() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        write_files(dir.path(), &["a.py"]);

        let projects = scan_repo(dir.path()).unwrap().projects;

        assert_eq!(projects[0].id, OTHER_PROJECT_ID);
        assert_eq!(projects[0].languages, vec![Language::Other]);
    }

    #[test]
    fn name_comes_from_package_json_and_keeps_a_scope() {
        assert_eq!(
            declared_name("package.json", r#"{"name": "pkg"}"#).as_deref(),
            Some("pkg")
        );
        assert_eq!(
            declared_name("package.json", r#"{"name": "@scope/pkg"}"#).as_deref(),
            Some("@scope/pkg")
        );
    }

    #[test]
    fn name_comes_from_cargo_package_table() {
        let text = "[package]\nname = \"core-lib\"\n\n[dependencies]\nname = \"other\"\n";

        assert_eq!(
            declared_name("Cargo.toml", text).as_deref(),
            Some("core-lib")
        );
    }

    #[test]
    fn name_comes_from_pyproject_project_then_poetry() {
        let project = "[project]\nname = \"pep621\"\n[tool.poetry]\nname = \"poetry\"\n";
        let poetry = "[tool.poetry]\nname = \"poetry\"\n";
        let empty_project = "[project]\nname = \"\"\n[tool.poetry]\nname = \"poetry\"\n";

        assert_eq!(
            declared_name("pyproject.toml", project).as_deref(),
            Some("pep621")
        );
        assert_eq!(
            declared_name("pyproject.toml", poetry).as_deref(),
            Some("poetry")
        );
        assert_eq!(
            declared_name("pyproject.toml", empty_project).as_deref(),
            Some("poetry")
        );
    }

    #[test]
    fn name_is_absent_when_the_manifest_has_no_usable_name() {
        let absent = [
            ("package.json", r#"{"version": "1"}"#),
            ("package.json", r#"{"name": ""}"#),
            ("package.json", r#"{"name": "  "}"#),
            ("package.json", r#"{"name": 7}"#),
            ("package.json", "not json"),
            ("Cargo.toml", "[package]\nname = 7\n"),
            ("Cargo.toml", "[workspace]\nmembers = []\n"),
            ("Cargo.toml", "not = = toml"),
            ("pyproject.toml", "[project]\nname = \"\"\n"),
            ("pyproject.toml", "[build-system]\nrequires = []\n"),
            ("setup.py", "setup(name='x')"),
            ("requirements.txt", "name"),
            ("App.csproj", "<Project />"),
        ];

        for (file_name, text) in absent {
            assert_eq!(declared_name(file_name, text), None, "{file_name}: {text}");
        }
    }

    #[test]
    fn project_name_is_the_declared_name_else_the_folder_name() {
        let dir = tempfile::Builder::new().prefix("scan-").tempdir().unwrap();
        let files = [
            ("web/package.json", r#"{"name": "@scope/pkg"}"#),
            ("web/a.ts", "x"),
            ("Cargo.toml", "[package]\nname = \"core-lib\"\n"),
            ("src/lib.rs", "x"),
            ("tools/pyproject.toml", "[project]\nname = 3\n"),
            ("tools/a.py", "x"),
            ("svc/Svc.csproj", "<Project />"),
            ("svc/a.cs", "x"),
        ];
        for (file, text) in files {
            let path = dir.path().join(file);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }

        let names: Vec<(String, String)> = scan_repo(dir.path())
            .unwrap()
            .projects
            .into_iter()
            .map(|p| (p.id, p.name))
            .collect();

        let pair = |id: &str, name: &str| (id.to_string(), name.to_string());
        assert_eq!(
            names,
            vec![
                pair("Cargo.toml", "core-lib"),
                pair("svc/Svc.csproj", "Svc"),
                pair("tools/pyproject.toml", "tools"),
                pair("web/package.json", "@scope/pkg"),
            ]
        );
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
                "dist/a.md",
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
