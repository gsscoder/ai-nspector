//! Python: `import` statements, top-level classes and functions.

use std::borrow::Cow;
use std::collections::BTreeSet;

use tree_sitter::Node;

use super::paths::{common_dir, join, parent_dir};
use super::{Symbol, SymbolKind, each_node, named_symbol, node_text};

pub(super) enum Import {
    /// `import a.b` (dotted module path)
    Module(String),
    /// `from <dots><module> import <names>`; `level` is the number of leading dots.
    From {
        level: usize,
        module: String,
        names: Vec<String>,
    },
}

pub(super) fn extract(root: Node, source: &str) -> (Vec<Import>, Vec<Symbol>) {
    (imports(root, source), symbols(root, source))
}

fn imports(root: Node, source: &str) -> Vec<Import> {
    let mut found = Vec::new();
    each_node(root, |node| match node.kind() {
        "import_statement" => {
            let mut cursor = node.walk();
            found.extend(
                node.children_by_field_name("name", &mut cursor)
                    .map(|name| Import::Module(module_name(name, source))),
            );
        }
        "import_from_statement" => found.extend(from_import(node, source)),
        _ => {}
    });
    found
}

fn from_import(node: Node, source: &str) -> Option<Import> {
    let module_node = node.child_by_field_name("module_name")?;
    let (level, module) = if module_node.kind() == "relative_import" {
        let mut cursor = module_node.walk();
        let mut prefix = "";
        let mut dotted = String::new();
        for child in module_node.named_children(&mut cursor) {
            match child.kind() {
                "import_prefix" => prefix = node_text(child, source),
                "dotted_name" => dotted = module_name(child, source),
                _ => {}
            }
        }
        (prefix.chars().filter(|c| *c == '.').count(), dotted)
    } else {
        (0, module_name(module_node, source))
    };
    let mut cursor = node.walk();
    let names = node
        .children_by_field_name("name", &mut cursor)
        .map(|name| module_name(name, source))
        .collect();
    Some(Import::From {
        level,
        module,
        names,
    })
}

/// Dotted path of a `dotted_name`, or of the module inside `x as y`.
fn module_name(node: Node, source: &str) -> String {
    let dotted = if node.kind() == "aliased_import" {
        node.child_by_field_name("name").unwrap_or(node)
    } else {
        node
    };
    node_text(dotted, source).split_whitespace().collect()
}

fn symbols(root: Node, source: &str) -> Vec<Symbol> {
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .filter_map(|node| symbol(node, source))
        .collect()
}

fn symbol(node: Node, source: &str) -> Option<Symbol> {
    let definition = if node.kind() == "decorated_definition" {
        node.child_by_field_name("definition")?
    } else {
        node
    };
    let kind = match definition.kind() {
        "class_definition" => SymbolKind::Class,
        "function_definition" => SymbolKind::Function,
        _ => return None,
    };
    named_symbol(definition, source, kind)
}

/// Matches imports against the Python files of one project.
///
/// An absolute import is tried against these import roots, in order:
/// 1. the common directory of all `.py` files of the project (the default project root),
/// 2. its `src/` folder,
/// 3. the first directory above the importing file that has no `__init__.py`
///    (covers a project whose files all sit inside one package).
///
/// `a.b` matches `<root>/a/b.py`, then `<root>/a/b/__init__.py`. In `from a.b import c`
/// the file `<root>/a/b/c.py` (or its package) is linked as well when it exists.
pub(super) struct Resolver<'files> {
    files: &'files BTreeSet<String>,
    base: String,
}

impl<'files> Resolver<'files> {
    pub(super) fn new(files: &'files BTreeSet<String>) -> Self {
        let python_files = files.iter().filter(|file| file.ends_with(".py"));
        Self {
            files,
            base: common_dir(python_files.map(String::as_str)),
        }
    }

    /// Project files that `imports` of `file` point to; may contain `file` itself and duplicates.
    pub(super) fn resolve(&self, file: &str, imports: &[Import]) -> Vec<String> {
        let mut targets = Vec::new();
        for import in imports {
            match import {
                Import::Module(module) => targets.extend(self.absolute_module(file, module)),
                Import::From {
                    level: 0,
                    module,
                    names,
                } => {
                    targets.extend(self.absolute_from(file, module, names));
                }
                Import::From {
                    level,
                    module,
                    names,
                } => {
                    targets.extend(self.relative_from(file, *level, module, names));
                }
            }
        }
        targets
    }

    fn absolute_module(&self, file: &str, module: &str) -> Option<String> {
        self.import_roots(file)
            .into_iter()
            .find_map(|root| self.module_file(&root, module))
    }

    fn absolute_from(&self, file: &str, module: &str, names: &[String]) -> Vec<String> {
        self.import_roots(file)
            .into_iter()
            .map(|root| self.resolve_from(&root, module, names))
            .find(|targets| !targets.is_empty())
            .unwrap_or_default()
    }

    fn relative_from(
        &self,
        file: &str,
        level: usize,
        module: &str,
        names: &[String],
    ) -> Vec<String> {
        let mut dir = parent_dir(file);
        for _ in 1..level {
            if dir.is_empty() {
                return Vec::new();
            }
            dir = parent_dir(dir);
        }
        self.resolve_from(dir, module, names)
    }

    /// The module file plus every imported name that is itself a module below it.
    fn resolve_from(&self, root: &str, module: &str, names: &[String]) -> Vec<String> {
        let module_dir = join(root, &module.replace('.', "/"));
        let module_dir = module_dir.trim_end_matches('/');
        self.module_file(root, module)
            .into_iter()
            .chain(
                names
                    .iter()
                    .filter_map(|name| self.module_file(module_dir, name)),
            )
            .collect()
    }

    fn import_roots(&self, file: &str) -> Vec<Cow<'_, str>> {
        let candidates = [
            Cow::Borrowed(self.base.as_str()),
            Cow::Owned(join(&self.base, "src")),
            Cow::Owned(self.package_root(file)),
        ];
        let mut roots = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if !roots.contains(&candidate) {
                roots.push(candidate);
            }
        }
        roots
    }

    fn package_root(&self, file: &str) -> String {
        let mut dir = parent_dir(file);
        while !dir.is_empty() && self.files.contains(&join(dir, "__init__.py")) {
            dir = parent_dir(dir);
        }
        dir.to_string()
    }

    /// File that defines the dotted `module` below `dir`; an empty module is the package `dir` itself.
    fn module_file(&self, dir: &str, module: &str) -> Option<String> {
        if module.is_empty() {
            return self.existing(&join(dir, "__init__.py"));
        }
        let path = join(dir, &module.replace('.', "/"));
        self.existing(&format!("{path}.py"))
            .or_else(|| self.existing(&format!("{path}/__init__.py")))
    }

    fn existing(&self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }
}
