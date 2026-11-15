//! C#: `using` directives, `namespace` declarations, top-level types.

use std::collections::BTreeMap;

use tree_sitter::Node;

use super::{Symbol, SymbolKind, named_symbol, node_text};

/// What one `.cs` file declares and uses, by namespace name.
#[derive(Default)]
pub(super) struct Unit {
    /// Full names of the namespaces declared in the file; nested block namespaces are composed.
    declared: Vec<String>,
    usings: Vec<Using>,
}

struct Using {
    /// Namespace, or for `using static` and `using A = ...` possibly a type inside a namespace.
    target: String,
    may_name_type: bool,
}

pub(super) fn extract(root: Node, source: &str) -> (Unit, Vec<Symbol>) {
    let mut extractor = Extractor {
        source,
        unit: Unit::default(),
        symbols: Vec::new(),
    };
    extractor.visit_members(root, "");
    (extractor.unit, extractor.symbols)
}

struct Extractor<'source> {
    source: &'source str,
    unit: Unit,
    symbols: Vec<Symbol>,
}

impl Extractor<'_> {
    /// Reads the members of a compilation unit, namespace body, `#if` branch or error node.
    fn visit_members(&mut self, container: Node, namespace: &str) {
        // A file-scoped namespace applies to the siblings that follow it.
        let mut namespace = namespace.to_string();
        let mut cursor = container.walk();
        for member in container.named_children(&mut cursor) {
            match member.kind() {
                "using_directive" => self.unit.usings.extend(using(member, self.source)),
                "file_scoped_namespace_declaration" => {
                    if let Some(full_name) = self.declare_namespace(member, &namespace) {
                        namespace = full_name;
                    }
                }
                "namespace_declaration" => {
                    if let Some(full_name) = self.declare_namespace(member, &namespace)
                        && let Some(body) = member.child_by_field_name("body")
                    {
                        self.visit_members(body, &full_name);
                    }
                }
                // Error recovery wraps the recognisable declarations of a broken file in an ERROR node.
                "preproc_if" | "preproc_elif" | "preproc_else" | "ERROR" => self.visit_members(member, &namespace),
                other => {
                    if let Some(kind) = type_kind(other) {
                        self.symbols.extend(named_symbol(member, self.source, kind));
                    }
                }
            }
        }
    }

    /// Records the full name of a namespace declaration and returns it.
    fn declare_namespace(&mut self, declaration: Node, enclosing: &str) -> Option<String> {
        let name = declaration.child_by_field_name("name")?;
        let full_name = match enclosing {
            "" => dotted_name(name, self.source),
            _ => format!("{enclosing}.{}", dotted_name(name, self.source)),
        };
        self.unit.declared.push(full_name.clone());
        Some(full_name)
    }
}

fn type_kind(node_kind: &str) -> Option<SymbolKind> {
    match node_kind {
        "class_declaration" => Some(SymbolKind::Class),
        "struct_declaration" => Some(SymbolKind::Struct),
        "interface_declaration" => Some(SymbolKind::Interface),
        "enum_declaration" => Some(SymbolKind::Enum),
        "record_declaration" => Some(SymbolKind::Record),
        _ => None,
    }
}

fn using(directive: Node, source: &str) -> Option<Using> {
    let alias = directive.child_by_field_name("name");
    let mut cursor = directive.walk();
    let children: Vec<Node> = directive.children(&mut cursor).collect();
    let target = children
        .iter()
        .filter(|child| child.is_named() && child.kind() != "comment")
        .rfind(|child| Some(child.id()) != alias.map(|alias| alias.id()))?;
    let is_static = children.iter().any(|child| child.kind() == "static");
    Some(Using {
        target: dotted_name(*target, source),
        may_name_type: alias.is_some() || is_static,
    })
}

/// Dotted name without whitespace and without a `global::` prefix.
fn dotted_name(node: Node, source: &str) -> String {
    let compact: String = node_text(node, source).split_whitespace().collect();
    compact.strip_prefix("global::").unwrap_or(&compact).to_string()
}

/// Namespace → files that declare it, built from all C# files of the project.
pub(super) struct NamespaceIndex {
    files_by_namespace: BTreeMap<String, Vec<String>>,
}

impl NamespaceIndex {
    pub(super) fn new<'unit>(units: impl Iterator<Item = (&'unit str, &'unit Unit)>) -> Self {
        let mut files_by_namespace: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (path, unit) in units {
            for namespace in &unit.declared {
                files_by_namespace.entry(namespace.clone()).or_default().push(path.to_string());
            }
        }
        Self { files_by_namespace }
    }

    /// Project files that declare the namespaces `unit` uses; may contain duplicates and the unit's own file.
    ///
    /// A `using static` or alias target that is not a namespace is read as a type and
    /// falls back to the namespace that contains it. External namespaces such as `System` match nothing.
    pub(super) fn resolve(&self, unit: &Unit) -> Vec<String> {
        unit.usings
            .iter()
            .flat_map(|using| {
                let direct = self.files_declaring(&using.target);
                match using.target.rsplit_once('.') {
                    Some((container, _)) if direct.is_empty() && using.may_name_type => self.files_declaring(container),
                    _ => direct,
                }
            })
            .cloned()
            .collect()
    }

    fn files_declaring(&self, namespace: &str) -> &[String] {
        self.files_by_namespace.get(namespace).map_or(&[], Vec::as_slice)
    }
}
