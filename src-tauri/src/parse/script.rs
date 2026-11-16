//! TypeScript and JavaScript: relative module specifiers, top-level declarations.

use std::collections::BTreeSet;

use tree_sitter::Node;

use super::paths::{join, normalize, parent_dir};
use super::{Symbol, SymbolKind, each_node, named_symbol, node_text};

const SOURCE_EXTENSIONS: [&str; 6] = ["ts", "tsx", "js", "jsx", "mjs", "cjs"];

/// Module specifiers found in `import`/`export ... from`/`require()`/`import()`,
/// in document order, together with the top-level symbols.
pub(super) fn extract(root: Node, source: &str) -> (Vec<String>, Vec<Symbol>) {
    (specifiers(root, source), symbols(root, source))
}

fn specifiers(root: Node, source: &str) -> Vec<String> {
    let mut found = Vec::new();
    each_node(root, |node| {
        let literal = match node.kind() {
            "import_statement" | "export_statement" | "import_require_clause" => {
                node.child_by_field_name("source")
            }
            "call_expression" => loaded_module(node, source),
            _ => None,
        };
        found.extend(
            literal
                .and_then(|node| string_value(node, source))
                .map(str::to_string),
        );
    });
    found
}

/// First argument of `require(...)` or `import(...)`.
fn loaded_module<'tree>(call: Node<'tree>, source: &str) -> Option<Node<'tree>> {
    let function = call.child_by_field_name("function")?;
    let loads_module = function.kind() == "import"
        || (function.kind() == "identifier" && node_text(function, source) == "require");
    if !loads_module {
        return None;
    }
    let arguments = call.child_by_field_name("arguments")?;
    arguments.named_child(0)
}

/// Text of a plain string literal without its quotes; `None` for any other node.
fn string_value<'a>(node: Node, source: &'a str) -> Option<&'a str> {
    if node.kind() != "string" {
        return None;
    }
    node_text(node, source)
        .strip_prefix(['"', '\''])?
        .strip_suffix(['"', '\''])
}

fn symbols(root: Node, source: &str) -> Vec<Symbol> {
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .filter_map(|node| symbol(node, source))
        .collect()
}

fn symbol(node: Node, source: &str) -> Option<Symbol> {
    if node.kind() != "export_statement" {
        return declaration(node, source);
    }
    if let Some(declared) = node.child_by_field_name("declaration") {
        return declaration(declared, source);
    }
    declaration(node.child_by_field_name("value")?, source)
}

fn declaration(node: Node, source: &str) -> Option<Symbol> {
    let kind = match node.kind() {
        "class_declaration" | "abstract_class_declaration" | "class" => SymbolKind::Class,
        "function_declaration"
        | "generator_function_declaration"
        | "function_expression"
        | "generator_function" => SymbolKind::Function,
        "interface_declaration" => SymbolKind::Interface,
        "enum_declaration" => SymbolKind::Enum,
        "type_alias_declaration" => SymbolKind::Type,
        _ => return None,
    };
    named_symbol(node, source, kind)
}

/// Project files that the specifiers of `file` point to.
///
/// Only relative specifiers (`./`, `../`) are internal; a bare name is a package. A
/// specifier is matched as written, then with a `.js`/`.jsx` ending mapped to its
/// TypeScript source (ESM style), then with each source extension appended, then as a
/// folder with an `index.*` file.
pub(super) fn resolve(files: &BTreeSet<String>, file: &str, specifiers: &[String]) -> Vec<String> {
    specifiers
        .iter()
        .filter(|specifier| is_relative(specifier))
        .filter_map(|specifier| normalize(&join(parent_dir(file), specifier)))
        .filter_map(|path| find_module(files, &path))
        .collect()
}

fn is_relative(specifier: &str) -> bool {
    matches!(specifier, "." | "..") || specifier.starts_with("./") || specifier.starts_with("../")
}

fn find_module(files: &BTreeSet<String>, path: &str) -> Option<String> {
    let existing = |candidate: &str| files.get(candidate).cloned();
    let with_extensions = |stem: &str| {
        SOURCE_EXTENSIONS
            .iter()
            .find_map(|ext| existing(&format!("{stem}.{ext}")))
    };

    existing(path)
        .or_else(|| {
            typescript_sources_of(path)
                .iter()
                .find_map(|candidate| existing(candidate))
        })
        .or_else(|| with_extensions(path))
        .or_else(|| with_extensions(&join(path, "index")))
}

/// ESM TypeScript writes the compiled name in imports: `./x.js` stands for `./x.ts` or `./x.tsx`.
fn typescript_sources_of(path: &str) -> Vec<String> {
    if let Some(stem) = path.strip_suffix(".js") {
        vec![format!("{stem}.ts"), format!("{stem}.tsx")]
    } else if let Some(stem) = path.strip_suffix(".jsx") {
        vec![format!("{stem}.tsx")]
    } else {
        Vec::new()
    }
}
