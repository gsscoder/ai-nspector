//! Go: import paths of `import` declarations, top-level types and functions.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use tree_sitter::Node;

use super::paths::{join, parent_dir};
use super::{Symbol, SymbolKind, each_node, named_symbol, node_text, read_utf8};

/// What one `.go` file says about other files of its module.
#[derive(Default)]
pub(super) struct Unit {
    /// Import paths as written in the source, one per `import_spec`; aliases are dropped.
    imports: Vec<String>,
}

pub(super) fn extract(root: Node, source: &str) -> (Unit, Vec<Symbol>) {
    let mut unit = Unit::default();
    let mut symbols = Vec::new();
    let mut cursor = root.walk();
    for node in root.named_children(&mut cursor) {
        match node.kind() {
            "import_declaration" => each_node(node, |spec| {
                if spec.kind() == "import_spec" {
                    unit.imports.extend(import_path(spec, source));
                }
            }),
            "function_declaration" | "method_declaration" => {
                symbols.extend(named_symbol(node, source, SymbolKind::Function));
            }
            "type_declaration" => {
                let mut cursor = node.walk();
                symbols.extend(
                    node.named_children(&mut cursor)
                        .filter_map(|spec| type_symbol(spec, source)),
                );
            }
            _ => {}
        }
    }
    (unit, symbols)
}

/// The path of an `import_spec` without its quotes; the alias (`name`, `_`, `.`) is another field.
fn import_path(spec: Node, source: &str) -> Option<String> {
    let path = unquote(node_text(spec.child_by_field_name("path")?, source));
    (!path.is_empty()).then(|| path.to_string())
}

fn type_symbol(node: Node, source: &str) -> Option<Symbol> {
    let kind = match node.kind() {
        "type_spec" => match node.child_by_field_name("type").map(|body| body.kind()) {
            Some("struct_type") => SymbolKind::Struct,
            Some("interface_type") => SymbolKind::Interface,
            _ => SymbolKind::Type,
        },
        "type_alias" => SymbolKind::Type,
        _ => return None,
    };
    named_symbol(node, source, kind)
}

/// Module path of every `go.mod` that sits in a folder above or beside a project `.go` file, and
/// the package files of every folder.
///
/// `go.mod` is not a source file, so it is not in the project file list: it is read from disk here,
/// once per folder, and `resolve` stays a pure function of this table.
pub(super) struct Modules<'a> {
    /// Repo-relative folder of a `go.mod` -> its `module` path.
    by_dir: BTreeMap<String, String>,
    /// Repo-relative folder -> the `.go` files directly in it, tests excluded, in path order.
    packages: BTreeMap<&'a str, Vec<&'a str>>,
}

impl<'a> Modules<'a> {
    pub(super) fn new(repo_root: &Path, files: &'a BTreeSet<String>) -> Self {
        let mut dirs = BTreeSet::new();
        let mut packages: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for file in files.iter().filter(|file| file.ends_with(".go")) {
            if !file.ends_with("_test.go") {
                packages.entry(parent_dir(file)).or_default().push(file);
            }
            let mut dir = parent_dir(file);
            while dirs.insert(dir) {
                dir = parent_dir(dir);
            }
        }
        let by_dir = dirs
            .into_iter()
            .filter_map(|dir| {
                let text = read_utf8(&repo_root.join(dir).join("go.mod"))?;
                Some((dir.to_string(), module_path(&text)?.to_string()))
            })
            .collect();
        Self { by_dir, packages }
    }

    /// Folder and module path of the `go.mod` that governs `file`: the nearest one above it.
    fn nearest(&self, file: &str) -> Option<(&str, &str)> {
        let mut dir = parent_dir(file);
        loop {
            if let Some((found, path)) = self.by_dir.get_key_value(dir) {
                return Some((found.as_str(), path.as_str()));
            }
            if dir.is_empty() {
                return None;
            }
            dir = parent_dir(dir);
        }
    }
}

/// Argument of the `module` directive of a `go.mod`, in the single-line form `module path` and in the
/// block form `module ( path )`; the path may be quoted, and `//` comments are ignored.
pub(crate) fn module_path(go_mod: &str) -> Option<&str> {
    let mut lines = go_mod
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code));
    while let Some(line) = lines.next() {
        let mut words = line.split_whitespace();
        if words.next() != Some("module") {
            continue;
        }
        let path = match words.next() {
            Some("(") => lines
                .map(str::trim)
                .find(|line| !line.is_empty())
                .filter(|&line| line != ")"),
            word => word,
        };
        return path.map(unquote).filter(|path| !path.is_empty());
    }
    None
}

/// An import or module path may be quoted with `"` or a backtick.
pub(crate) fn unquote(path: &str) -> &str {
    path.trim_matches(['"', '`'])
}

/// Project files that the imports of `file` point to.
///
/// An import that equals the module path of the governing `go.mod`, or continues it after a `/`
/// (`example.com/m/v2/store` for module `example.com/m/v2`), names a package folder; the targets are
/// the `.go` files directly in that folder, tests excluded. Any other import is external and gives no link.
///
/// ponytail: files of the same package do not link; links between modules of a `go.work` are not
/// made; build tags, `replace` lines and `vendor/` are not read, and an import into a nested module
/// resolves only against the nearest `go.mod`.
pub(super) fn resolve(modules: &Modules, file: &str, unit: &Unit) -> Vec<String> {
    let Some((module_dir, module_path)) = modules.nearest(file) else {
        return Vec::new();
    };
    unit.imports
        .iter()
        .filter_map(|import| {
            let folder = match import.strip_prefix(module_path)? {
                "" => module_dir.to_string(),
                rest => join(module_dir, rest.strip_prefix('/')?),
            };
            modules.packages.get(folder.as_str())
        })
        .flatten()
        .map(|file| file.to_string())
        .collect()
}
