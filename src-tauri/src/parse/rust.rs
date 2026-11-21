//! Rust: `mod foo;` declarations, `crate::`/`self::`/`super::` paths of `use`, top-level items.

use std::collections::BTreeSet;

use tree_sitter::Node;

use super::paths::{join, parent_dir};
use super::{Symbol, SymbolKind, named_symbol, node_text};

/// What one `.rs` file says about other files of its crate.
#[derive(Default)]
pub(super) struct Unit {
    /// Names of the file modules declared with `mod foo;`.
    modules: Vec<String>,
    /// Internal `use` paths (`crate::a::b`), one per imported item; groups are expanded.
    uses: Vec<String>,
}

pub(super) fn extract(root: Node, source: &str) -> (Unit, Vec<Symbol>) {
    let mut unit = Unit::default();
    collect_links(root, source, 0, &mut unit);
    (unit, symbols(root, source))
}

/// Reads the items of the file or of an inline `mod x { ... }` body; `depth` counts the inline modules around them.
///
/// ponytail: `use` inside function bodies is not read; `mod foo;` and `self::`/`super::` paths
/// inside inline modules are ignored (their base is not the file); `#[path]` attributes and
/// modules made by macros are not followed.
fn collect_links(container: Node, source: &str, depth: usize, unit: &mut Unit) {
    let mut cursor = container.walk();
    for item in container.named_children(&mut cursor) {
        match item.kind() {
            "mod_item" => match (
                item.child_by_field_name("body"),
                item.child_by_field_name("name"),
            ) {
                (Some(body), _) => collect_links(body, source, depth + 1, unit),
                (None, Some(name)) if depth == 0 => {
                    unit.modules.push(node_text(name, source).to_string());
                }
                _ => {}
            },
            "use_declaration" => {
                let Some(argument) = item.child_by_field_name("argument") else {
                    continue;
                };
                let mut paths = Vec::new();
                use_paths(argument, source, "", &mut paths);
                unit.uses
                    .extend(paths.into_iter().filter(|path| is_internal(path, depth)));
            }
            _ => {}
        }
    }
}

/// Appends one `::` path per item named by a `use` tree; `prefix` is the path of the enclosing group.
/// Glob `*` and `as` aliases are dropped.
fn use_paths(node: Node, source: &str, prefix: &str, paths: &mut Vec<String>) {
    match node.kind() {
        "scoped_use_list" => {
            if let (Some(path), Some(list)) = (
                node.child_by_field_name("path"),
                node.child_by_field_name("list"),
            ) {
                let prefix = join_path(prefix, path, source);
                use_paths(list, source, &prefix, paths);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                use_paths(child, source, prefix, paths);
            }
        }
        "use_as_clause" => {
            if let Some(path) = node.child_by_field_name("path") {
                use_paths(path, source, prefix, paths);
            }
        }
        "use_wildcard" => match node.named_child(0) {
            Some(path) => use_paths(path, source, prefix, paths),
            None => paths.push(prefix.to_string()),
        },
        // `{self, ...}` imports the module of the group itself.
        "self" if !prefix.is_empty() => paths.push(prefix.to_string()),
        "identifier" | "scoped_identifier" | "crate" | "self" | "super" => {
            paths.push(join_path(prefix, node, source));
        }
        _ => {}
    }
}

fn join_path(prefix: &str, node: Node, source: &str) -> String {
    let text: String = node_text(node, source).split_whitespace().collect();
    if prefix.is_empty() {
        text
    } else {
        format!("{prefix}::{text}")
    }
}

/// `crate::` always points into the crate; `self::` and `super::` only when the `use` is not inside an inline module.
fn is_internal(path: &str, depth: usize) -> bool {
    match path.split("::").next() {
        Some("crate") => true,
        Some("self" | "super") => depth == 0,
        _ => false,
    }
}

fn symbols(root: Node, source: &str) -> Vec<Symbol> {
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .filter_map(|node| symbol(node, source))
        .collect()
}

fn symbol(node: Node, source: &str) -> Option<Symbol> {
    let kind = match node.kind() {
        "function_item" => SymbolKind::Function,
        "struct_item" => SymbolKind::Struct,
        "enum_item" => SymbolKind::Enum,
        "trait_item" => SymbolKind::Interface,
        "type_item" => SymbolKind::Type,
        _ => return None,
    };
    named_symbol(node, source, kind)
}

/// Project files that the `mod` declarations and `use` paths of `file` point to.
///
/// A `mod foo;` is looked up as `foo.rs` or `foo/mod.rs` in the module directory of `file`: its own
/// folder for `lib.rs`, `main.rs` and `mod.rs`, otherwise the folder named like the file (`a.rs` -> `a/`).
/// A `use` path is cut after `crate`, `self` or `super`; the longest leading run of the remaining
/// segments that names a file (`a/b.rs` or `a/b/mod.rs`) is the target, the rest is an item name.
/// The crate root directory is the nearest ancestor folder of `file` that holds `lib.rs` or `main.rs`.
///
/// ponytail: `src/bin`, `tests/` and `examples/` files resolve `crate::` against the nearest `src/`
/// without any crate-name handling, links across crates of a workspace are not made, and an item
/// defined directly in the base module file (`use crate::Item`) gives no link.
pub(super) fn resolve(files: &BTreeSet<String>, file: &str, unit: &Unit) -> Vec<String> {
    let module_dir = module_dir(file);
    let declared = unit
        .modules
        .iter()
        .filter_map(|name| module_file(files, module_dir, name));
    let used = unit
        .uses
        .iter()
        .filter_map(|path| use_target(files, file, module_dir, path));
    declared.chain(used).collect()
}

/// Folder that holds the child modules of `file`.
fn module_dir(file: &str) -> &str {
    let name = file.rsplit('/').next().unwrap_or(file);
    if matches!(name, "lib.rs" | "main.rs" | "mod.rs") {
        parent_dir(file)
    } else {
        file.rsplit_once('.').map_or(file, |(stem, _)| stem)
    }
}

fn crate_dir<'file>(files: &BTreeSet<String>, file: &'file str) -> Option<&'file str> {
    let mut dir = parent_dir(file);
    loop {
        if files.contains(&join(dir, "lib.rs")) || files.contains(&join(dir, "main.rs")) {
            return Some(dir);
        }
        if dir.is_empty() {
            return None;
        }
        dir = parent_dir(dir);
    }
}

fn use_target(
    files: &BTreeSet<String>,
    file: &str,
    module_dir: &str,
    path: &str,
) -> Option<String> {
    let mut segments = path.split("::").peekable();
    let mut base = match segments.next()? {
        "crate" => crate_dir(files, file)?,
        "self" => module_dir,
        "super" => parent_dir(module_dir),
        _ => return None,
    };
    while segments.next_if_eq(&"super").is_some() {
        base = parent_dir(base);
    }
    let rest: Vec<&str> = segments.collect();
    (1..=rest.len())
        .rev()
        .find_map(|len| module_file(files, base, &rest[..len].join("/")))
}

/// `<dir>/<module>.rs` or `<dir>/<module>/mod.rs`; `module` may hold several `/`-joined segments.
fn module_file(files: &BTreeSet<String>, dir: &str, module: &str) -> Option<String> {
    let path = join(dir, module);
    files
        .get(&format!("{path}.rs"))
        .or_else(|| files.get(&format!("{path}/mod.rs")))
        .cloned()
}
