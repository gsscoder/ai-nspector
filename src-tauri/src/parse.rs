//! Import graph and top-level symbols of one project, extracted with tree-sitter.
//!
//! Resolution is syntactic and approximate: it only matches import text against the
//! file list of the project and never runs a language tool chain.

mod csharp;
mod paths;
mod python;
mod rust;
mod script;
#[cfg(test)]
mod tests;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

#[cfg(test)]
use ignore::WalkBuilder;
use serde::Serialize;
use tree_sitter::{Language, Node, Parser};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStructure {
    pub files: Vec<FileInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    /// Repo-relative path with forward slashes.
    pub path: String,
    /// Repo-relative paths of other files of the same project; sorted, unique.
    pub imports: Vec<String>,
    pub symbols: Vec<Symbol>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Class,
    Function,
    Interface,
    Enum,
    Struct,
    Record,
    Type,
}

/// Larger files (generated or minified code) are listed but not parsed; same limit as the code view.
const MAX_PARSE_BYTES: u64 = 1_000_000;

/// Directory names that never hold project sources; shared by the debug file walk.
#[cfg(test)]
const SKIPPED_DIRS: [&str; 8] = [
    "bin",
    "obj",
    "node_modules",
    ".venv",
    "dist",
    "target",
    "__pycache__",
    ".ainspector",
];

/// Parses every file of ONE project and links the files that import each other.
///
/// `files` are repo-relative paths; the language comes from the extension. Files with
/// another extension, unreadable files, files over `MAX_PARSE_BYTES` and files that are not UTF-8 yield an empty
/// `FileInfo`. Broken syntax yields whatever tree-sitter could still recognise.
/// The result is sorted by path. `Err` means a bad argument (`repo_root` is not a
/// directory, a path leaves the repo) or a grammar that cannot be loaded.
pub fn analyze_project(repo_root: &Path, files: &[String]) -> Result<ProjectStructure, String> {
    if !repo_root.is_dir() {
        return Err(format!(
            "repo root is not a directory: {}",
            repo_root.display()
        ));
    }
    let project_files = normalize_project_files(files)?;
    let parsed = project_files
        .iter()
        .map(|path| parse_file(repo_root, path))
        .collect::<Result<Vec<_>, _>>()?;

    let python_resolver = python::Resolver::new(&project_files);
    let namespaces =
        csharp::NamespaceIndex::new(parsed.iter().filter_map(|file| match &file.links {
            Links::CSharp(unit) => Some((file.path.as_str(), unit)),
            _ => None,
        }));

    let files = parsed
        .into_iter()
        .map(|file| {
            let targets = match &file.links {
                Links::None => Vec::new(),
                Links::Python(imports) => python_resolver.resolve(&file.path, imports),
                Links::Script(specifiers) => {
                    script::resolve(&project_files, &file.path, specifiers)
                }
                Links::CSharp(unit) => namespaces.resolve(unit),
                Links::Rust(unit) => rust::resolve(&project_files, &file.path, unit),
            };
            let imports: BTreeSet<String> = targets
                .into_iter()
                .filter(|target| *target != file.path)
                .collect();
            FileInfo {
                path: file.path,
                imports: imports.into_iter().collect(),
                symbols: file.symbols,
            }
        })
        .collect();
    Ok(ProjectStructure { files })
}

fn normalize_project_files(files: &[String]) -> Result<BTreeSet<String>, String> {
    files
        .iter()
        .map(|file| {
            let path = file.replace('\\', "/");
            let is_relative = Path::new(&path).components().all(|part| {
                matches!(
                    part,
                    Component::Normal(_) | Component::CurDir | Component::ParentDir
                )
            });
            is_relative
                .then(|| paths::normalize(&path))
                .flatten()
                .filter(|path| !path.is_empty())
                .ok_or_else(|| format!("file path is not inside the repo: {file}"))
        })
        .collect()
}

/// The one table of source extensions with a grammar; `scan` reads it too.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Dialect {
    Python,
    TypeScript,
    Tsx,
    JavaScript,
    CSharp,
    Rust,
}

impl Dialect {
    pub(crate) fn from_path(path: &str) -> Option<Self> {
        let extension = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "py" | "pyi" => Some(Self::Python),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "cs" => Some(Self::CSharp),
            "rs" => Some(Self::Rust),
            _ => None,
        }
    }

    fn language(self) -> Language {
        match self {
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }
}

/// What a file says about other files, before it is matched against the project.
enum Links {
    None,
    Python(Vec<python::Import>),
    Script(Vec<String>),
    CSharp(csharp::Unit),
    Rust(rust::Unit),
}

struct ParsedFile {
    path: String,
    symbols: Vec<Symbol>,
    links: Links,
}

fn parse_file(repo_root: &Path, path: &str) -> Result<ParsedFile, String> {
    let empty = || ParsedFile {
        path: path.to_string(),
        symbols: Vec::new(),
        links: Links::None,
    };
    let Some(dialect) = Dialect::from_path(path) else {
        return Ok(empty());
    };
    let Some(text) = read_utf8(&repo_root.join(path)) else {
        return Ok(empty());
    };
    // A BOM is not part of the code; C# files from Visual Studio often start with one.
    let source = text.trim_start_matches('\u{feff}');

    let mut parser = Parser::new();
    parser
        .set_language(&dialect.language())
        .map_err(|error| format!("cannot load the {dialect:?} grammar: {error}"))?;
    let Some(tree) = parser.parse(source, None) else {
        return Ok(empty());
    };
    let root = tree.root_node();

    let (links, symbols) = match dialect {
        Dialect::Python => {
            let (imports, symbols) = python::extract(root, source);
            (Links::Python(imports), symbols)
        }
        Dialect::TypeScript | Dialect::Tsx | Dialect::JavaScript => {
            let (specifiers, symbols) = script::extract(root, source);
            (Links::Script(specifiers), symbols)
        }
        Dialect::CSharp => {
            let (unit, symbols) = csharp::extract(root, source);
            (Links::CSharp(unit), symbols)
        }
        Dialect::Rust => {
            let (unit, symbols) = rust::extract(root, source);
            (Links::Rust(unit), symbols)
        }
    };
    Ok(ParsedFile {
        path: path.to_string(),
        symbols,
        links,
    })
}

/// `None` for a missing, unreadable, oversized or non-UTF-8 file: such a file has no structure to report.
fn read_utf8(path: &Path) -> Option<String> {
    if fs::metadata(path).ok()?.len() > MAX_PARSE_BYTES {
        return None;
    }
    String::from_utf8(fs::read(path).ok()?).ok()
}

fn node_text<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}

/// Symbol named by the `name` field of `node`; `None` when the name is absent or empty
/// (error recovery can produce a missing name).
fn named_symbol(node: Node, source: &str, kind: SymbolKind) -> Option<Symbol> {
    let name = node_text(node.child_by_field_name("name")?, source);
    (!name.is_empty()).then(|| Symbol {
        name: name.to_string(),
        kind,
    })
}

/// Visits `root` and all its descendants in document order, without recursion.
fn each_node<'tree>(root: Node<'tree>, mut visit: impl FnMut(Node<'tree>)) {
    let mut cursor = root.walk();
    loop {
        visit(cursor.node());
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

/// Test helper: the real file lists come from `scan`.
#[cfg(test)]
/// Repo-relative paths of the source files below `project_dir`, honouring `.gitignore`.
fn collect_source_files(repo_root: &Path, project_dir: &str) -> Result<Vec<String>, String> {
    let project_root = repo_root.join(project_dir);
    let walker = WalkBuilder::new(&project_root)
        .filter_entry(|entry| entry.depth() == 0 || !is_skipped_dir(entry))
        .build();

    let mut files = Vec::new();
    for entry in walker {
        let entry =
            entry.map_err(|error| format!("cannot walk {}: {error}", project_root.display()))?;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(repo_root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        if Dialect::from_path(&relative).is_some() {
            files.push(relative);
        }
    }
    Ok(files)
}

#[cfg(test)]
fn is_skipped_dir(entry: &ignore::DirEntry) -> bool {
    entry.file_type().is_some_and(|kind| kind.is_dir())
        && entry
            .file_name()
            .to_str()
            .is_some_and(|name| SKIPPED_DIRS.contains(&name))
}
