use std::fs;
use std::path::{Path, PathBuf};

use super::SymbolKind::{Class, Enum, Function, Interface, Record, Struct, Type};
use super::{ProjectStructure, SymbolKind, analyze_project, collect_source_files};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/parse")
        .join(name)
}

fn analyze_fixture(name: &str) -> ProjectStructure {
    let root = fixture(name);
    let files = collect_source_files(&root, "").expect("fixture files");
    analyze_project(&root, &files).expect("fixture analysis")
}

fn analyze_paths(root: &Path, files: &[&str]) -> Result<ProjectStructure, String> {
    let files: Vec<String> = files.iter().map(|file| file.to_string()).collect();
    analyze_project(root, &files)
}

fn edges(structure: &ProjectStructure) -> Vec<(&str, Vec<&str>)> {
    structure
        .files
        .iter()
        .map(|file| {
            (
                file.path.as_str(),
                file.imports.iter().map(String::as_str).collect(),
            )
        })
        .collect()
}

fn symbols<'a>(structure: &'a ProjectStructure, path: &str) -> Vec<(&'a str, SymbolKind)> {
    structure
        .files
        .iter()
        .find(|file| file.path == path)
        .unwrap_or_else(|| panic!("{path} is not in the structure"))
        .symbols
        .iter()
        .map(|symbol| (symbol.name.as_str(), symbol.kind))
        .collect()
}

#[test]
fn python_edges_cover_src_layout_and_relative_imports() {
    let structure = analyze_fixture("python");

    assert_eq!(
        edges(&structure),
        vec![
            ("src/app/__init__.py", vec!["src/app/core.py"]),
            (
                "src/app/core.py",
                vec![
                    "src/app/__init__.py",
                    "src/app/helpers.py",
                    "src/app/models.py",
                    "src/app/util.py",
                ]
            ),
            ("src/app/helpers.py", vec!["src/app/util.py"]),
            ("src/app/models.py", vec![]),
            (
                "src/app/sub/deep.py",
                vec![
                    "src/app/__init__.py",
                    "src/app/models.py",
                    "src/app/util.py"
                ]
            ),
            ("src/app/util.py", vec![]),
            (
                "tests/test_core.py",
                vec![
                    "src/app/__init__.py",
                    "src/app/core.py",
                    "src/app/models.py"
                ]
            ),
        ]
    );
}

#[test]
fn python_symbols_are_top_level_classes_and_functions() {
    let structure = analyze_fixture("python");

    assert_eq!(
        symbols(&structure, "src/app/core.py"),
        vec![("run", Function)]
    );
    assert_eq!(
        symbols(&structure, "src/app/models.py"),
        vec![("User", Class), ("Admin", Class)]
    );
    assert_eq!(
        symbols(&structure, "src/app/util.py"),
        vec![("slug", Function), ("fetch", Function), ("Box", Class)]
    );
    assert_eq!(symbols(&structure, "src/app/__init__.py"), vec![]);
}

#[test]
fn python_package_inside_project_dir_resolves_through_package_root() {
    let root = fixture("python");
    // Only the package is passed: the common directory is `src/app`, not the import root `src`.
    let structure = analyze_paths(
        &root,
        &["src/app/__init__.py", "src/app/core.py", "src/app/util.py"],
    )
    .unwrap();

    assert_eq!(
        edges(&structure),
        vec![
            ("src/app/__init__.py", vec!["src/app/core.py"]),
            (
                "src/app/core.py",
                vec!["src/app/__init__.py", "src/app/util.py"]
            ),
            ("src/app/util.py", vec![]),
        ]
    );
}

#[test]
fn typescript_edges_cover_index_files_js_specifiers_and_externals() {
    let structure = analyze_fixture("typescript");

    assert_eq!(
        edges(&structure),
        vec![
            ("src/greeter.ts", vec!["src/models.ts"]),
            (
                "src/index.ts",
                vec![
                    "src/greeter.ts",
                    "src/lazy.tsx",
                    "src/lib/index.ts",
                    "src/models.ts"
                ]
            ),
            ("src/lazy.tsx", vec!["src/greeter.ts"]),
            ("src/legacy.cjs", vec![]),
            ("src/lib/index.ts", vec!["src/legacy.cjs"]),
            ("src/models.ts", vec![]),
        ]
    );
}

#[test]
fn typescript_symbols_include_exported_and_default_declarations() {
    let structure = analyze_fixture("typescript");

    assert_eq!(
        symbols(&structure, "src/index.ts"),
        vec![("main", Function), ("lazy", Function)]
    );
    assert_eq!(
        symbols(&structure, "src/greeter.ts"),
        vec![("Greeter", Class), ("greet", Function)]
    );
    assert_eq!(
        symbols(&structure, "src/models.ts"),
        vec![
            ("User", Interface),
            ("Role", Enum),
            ("Id", Type),
            ("Base", Class)
        ]
    );
    assert_eq!(symbols(&structure, "src/lazy.tsx"), vec![("Lazy", Class)]);
    assert_eq!(
        symbols(&structure, "src/legacy.cjs"),
        vec![("old", Function)]
    );
}

#[test]
fn csharp_edges_follow_using_directives_to_declaring_files() {
    let structure = analyze_fixture("csharp");

    assert_eq!(
        edges(&structure),
        vec![
            ("Core/Entity.cs", vec![]),
            ("Core/Money.cs", vec![]),
            ("GlobalUsings.cs", vec!["Util/Maths.cs"]),
            (
                "Program.cs",
                vec![
                    "Core/Entity.cs",
                    "Core/Money.cs",
                    "Services/BillingService.cs",
                    "Util/Maths.cs"
                ]
            ),
            (
                "Services/BillingService.cs",
                vec!["Core/Entity.cs", "Core/Money.cs"]
            ),
            ("Util/Maths.cs", vec![]),
        ]
    );
}

#[test]
fn csharp_symbols_are_types_in_any_namespace_style() {
    let structure = analyze_fixture("csharp");

    assert_eq!(symbols(&structure, "Program.cs"), vec![("Program", Class)]);
    assert_eq!(
        symbols(&structure, "Core/Entity.cs"),
        vec![("Entity", Class), ("Order", Record), ("Status", Enum)]
    );
    // This file starts with a byte order mark.
    assert_eq!(
        symbols(&structure, "Core/Money.cs"),
        vec![("Money", Struct)]
    );
    assert_eq!(
        symbols(&structure, "Services/BillingService.cs"),
        vec![("BillingService", Class), ("IBilling", Interface)]
    );
    assert_eq!(symbols(&structure, "Util/Maths.cs"), vec![("Maths", Class)]);
    assert_eq!(symbols(&structure, "GlobalUsings.cs"), vec![]);
}

#[test]
fn broken_syntax_still_yields_what_was_recognised() {
    let structure = analyze_fixture("broken");

    assert_eq!(
        edges(&structure),
        vec![
            ("Broken.cs", vec![]),
            ("broken.py", vec!["good.py"]),
            ("broken.ts", vec!["good.ts"]),
            ("good.py", vec![]),
            ("good.ts", vec![]),
        ]
    );
    assert_eq!(symbols(&structure, "Broken.cs"), vec![("Fine", Class)]);
    assert_eq!(symbols(&structure, "broken.py"), vec![("ok", Function)]);
    assert_eq!(symbols(&structure, "broken.ts"), vec![("ok", Function)]);
}

#[test]
fn unreadable_files_give_empty_file_info() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("binary.py"),
        [0xff, 0xfe, 0x00, b'i', b'm', 0xc3, 0x28],
    )
    .unwrap();
    fs::write(dir.path().join("ok.py"), "import binary\n").unwrap();

    let structure = analyze_paths(
        dir.path(),
        &["binary.py", "ok.py", "missing.py", "notes.txt"],
    )
    .unwrap();

    assert_eq!(
        edges(&structure),
        vec![
            ("binary.py", vec![]),
            ("missing.py", vec![]),
            ("notes.txt", vec![]),
            ("ok.py", vec!["binary.py"]),
        ]
    );
    assert_eq!(symbols(&structure, "binary.py"), vec![]);
}

#[test]
fn oversized_files_are_listed_but_not_parsed() {
    let dir = tempfile::tempdir().unwrap();
    let padding = "#".repeat(super::MAX_PARSE_BYTES as usize);
    fs::write(
        dir.path().join("big.py"),
        format!("def big(): pass\n{padding}"),
    )
    .unwrap();
    fs::write(dir.path().join("ok.py"), "import big\n").unwrap();

    let structure = analyze_paths(dir.path(), &["big.py", "ok.py"]).unwrap();

    assert_eq!(
        edges(&structure),
        vec![("big.py", vec![]), ("ok.py", vec!["big.py"])]
    );
    assert_eq!(symbols(&structure, "big.py"), vec![]);
}

#[test]
fn imports_are_unique_sorted_and_never_the_file_itself() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("a.js"),
        "import './b.js';\nimport './b';\nrequire('./a');\nrequire('./c.js');\nconst x = require(dynamic);\nimport('./b');\n",
    )
    .unwrap();
    fs::write(dir.path().join("b.js"), "").unwrap();
    fs::write(dir.path().join("c.js"), "").unwrap();

    let structure = analyze_paths(dir.path(), &["c.js", "a.js", "b.js", "a.js"]).unwrap();

    assert_eq!(
        edges(&structure),
        vec![
            ("a.js", vec!["b.js", "c.js"]),
            ("b.js", vec![]),
            ("c.js", vec![])
        ]
    );
}

#[test]
fn backslash_paths_are_accepted_and_reported_with_forward_slashes() {
    let root = fixture("python");

    let structure = analyze_paths(
        &root,
        &[
            "src\\app\\__init__.py",
            "src\\app\\core.py",
            "src\\app\\util.py",
        ],
    )
    .unwrap();

    assert_eq!(
        edges(&structure),
        vec![
            ("src/app/__init__.py", vec!["src/app/core.py"]),
            (
                "src/app/core.py",
                vec!["src/app/__init__.py", "src/app/util.py"]
            ),
            ("src/app/util.py", vec![]),
        ]
    );
}

#[test]
fn paths_outside_the_repo_and_a_bad_root_are_errors() {
    let root = fixture("python");

    assert!(
        analyze_paths(&root, &["../escape.py"])
            .unwrap_err()
            .contains("../escape.py")
    );
    assert!(analyze_paths(&root, &["/etc/passwd.py"]).is_err());
    assert!(
        analyze_paths(&root.join("missing"), &[])
            .unwrap_err()
            .contains("not a directory")
    );
}

#[test]
fn symbols_serialize_with_lowercase_kind() {
    let structure = analyze_fixture("typescript");

    let json = serde_json::to_value(&structure).unwrap();
    let models = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "src/models.ts")
        .unwrap();

    assert_eq!(models["imports"], serde_json::json!([]));
    assert_eq!(
        models["symbols"][2],
        serde_json::json!({ "name": "Id", "kind": "type" })
    );
}

#[test]
fn debug_walk_skips_build_folders_and_other_languages() {
    let dir = tempfile::tempdir().unwrap();
    for file in [
        "app/a.py",
        "app/node_modules/x.js",
        "app/obj/y.cs",
        "app/readme.md",
        "other/b.py",
    ] {
        let path = dir.path().join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    assert_eq!(
        collect_source_files(dir.path(), "app").unwrap(),
        vec!["app/a.py"]
    );
}

/// Writes `(path, content)` pairs below `root` and analyzes all of them as one project.
fn analyze_sources(root: &Path, sources: &[(&str, &str)]) -> ProjectStructure {
    for (path, content) in sources {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let paths: Vec<&str> = sources.iter().map(|(path, _)| *path).collect();
    analyze_paths(root, &paths).unwrap()
}

#[test]
fn rust_symbols_are_top_level_items() {
    let dir = tempfile::tempdir().unwrap();
    let structure = analyze_sources(
        dir.path(),
        &[(
            "lib.rs",
            "pub fn run() {}\n\
             pub(crate) struct Config { a: u8 }\n\
             enum Mode { A, B }\n\
             #[derive(Debug)]\n\
             pub trait Store { fn get(&self); }\n\
             type Id = u64;\n\
             impl Config { fn new() -> Self { todo!() } }\n\
             const LIMIT: u8 = 1;\n\
             mod inline { pub fn hidden() {} }\n",
        )],
    );

    assert_eq!(
        symbols(&structure, "lib.rs"),
        vec![
            ("run", Function),
            ("Config", Struct),
            ("Mode", Enum),
            ("Store", Interface),
            ("Id", Type),
        ]
    );
}

#[test]
fn rust_mod_statements_link_foo_rs_and_foo_mod_rs_in_the_module_directory() {
    let dir = tempfile::tempdir().unwrap();
    let structure = analyze_sources(
        dir.path(),
        &[
            (
                "src/lib.rs",
                "mod a;\nmod b;\nmod missing;\nmod inline { }\n",
            ),
            ("src/a.rs", "mod c;\n"),
            ("src/a/c.rs", ""),
            ("src/b/mod.rs", "mod d;\n"),
            ("src/b/d.rs", ""),
            ("src/inline.rs", ""),
        ],
    );

    assert_eq!(
        edges(&structure),
        vec![
            ("src/a.rs", vec!["src/a/c.rs"]),
            ("src/a/c.rs", vec![]),
            ("src/b/d.rs", vec![]),
            ("src/b/mod.rs", vec!["src/b/d.rs"]),
            ("src/inline.rs", vec![]),
            ("src/lib.rs", vec!["src/a.rs", "src/b/mod.rs"]),
        ]
    );
}

#[test]
fn rust_use_paths_resolve_against_crate_self_and_super_and_drop_the_own_file() {
    let dir = tempfile::tempdir().unwrap();
    let structure = analyze_sources(
        dir.path(),
        &[
            ("src/lib.rs", ""),
            (
                "src/a.rs",
                "use crate::b::Thing;\nuse self::c::Inner;\nuse crate::a::inner::Item;\n",
            ),
            (
                "src/a/c.rs",
                "use super::super::b::f;\nuse super::sibling::g;\n",
            ),
            ("src/a/sibling.rs", ""),
            ("src/b.rs", ""),
            ("src/d/mod.rs", "pub use super::b::Thing;\n"),
        ],
    );

    assert_eq!(
        edges(&structure),
        vec![
            ("src/a.rs", vec!["src/a/c.rs", "src/b.rs"]),
            ("src/a/c.rs", vec!["src/a/sibling.rs", "src/b.rs"]),
            ("src/a/sibling.rs", vec![]),
            ("src/b.rs", vec![]),
            ("src/d/mod.rs", vec!["src/b.rs"]),
            ("src/lib.rs", vec![]),
        ]
    );
}

#[test]
fn rust_grouped_use_gives_one_link_per_path_and_longest_file_prefix_wins() {
    let dir = tempfile::tempdir().unwrap();
    let structure = analyze_sources(
        dir.path(),
        &[
            ("src/main.rs", ""),
            (
                "src/user.rs",
                "use crate::{a::{X, Y}, b::{self, Z as W}, c::*, d::sub::{Deep, deeper::Item}};\n",
            ),
            ("src/a.rs", ""),
            ("src/b.rs", ""),
            ("src/c/mod.rs", ""),
            ("src/d/sub.rs", ""),
            ("src/d/sub/deeper.rs", ""),
        ],
    );

    assert_eq!(
        edges(&structure)
            .into_iter()
            .find(|(path, _)| *path == "src/user.rs")
            .unwrap()
            .1,
        vec![
            "src/a.rs",
            "src/b.rs",
            "src/c/mod.rs",
            "src/d/sub.rs",
            "src/d/sub/deeper.rs"
        ]
    );
}

#[test]
fn rust_external_crates_give_no_links_but_inline_module_crate_uses_resolve() {
    let dir = tempfile::tempdir().unwrap();
    let structure = analyze_sources(
        dir.path(),
        &[
            ("src/lib.rs", ""),
            (
                "src/a.rs",
                "use std::fs;\nuse serde::{Serialize, Deserialize};\nuse b::helper;\n\
                 #[cfg(test)]\nmod tests {\n    use super::*;\n    use self::x::y;\n    use crate::b::Thing;\n}\n",
            ),
            ("src/b.rs", ""),
            ("src/std.rs", ""),
        ],
    );

    assert_eq!(
        edges(&structure),
        vec![
            ("src/a.rs", vec!["src/b.rs"]),
            ("src/b.rs", vec![]),
            ("src/lib.rs", vec![]),
            ("src/std.rs", vec![]),
        ]
    );
}

#[test]
fn rust_crate_root_is_the_nearest_ancestor_with_lib_or_main() {
    let dir = tempfile::tempdir().unwrap();
    let structure = analyze_sources(
        dir.path(),
        &[
            ("crates/x/src/lib.rs", "mod a;\n"),
            ("crates/x/src/a.rs", "use crate::b::f;\nuse y::b::f;\n"),
            ("crates/x/src/b.rs", ""),
            ("crates/y/src/main.rs", ""),
            ("crates/y/src/b.rs", ""),
            ("loose/z.rs", "use crate::b::f;\n"),
        ],
    );

    assert_eq!(
        edges(&structure),
        vec![
            ("crates/x/src/a.rs", vec!["crates/x/src/b.rs"]),
            ("crates/x/src/b.rs", vec![]),
            ("crates/x/src/lib.rs", vec!["crates/x/src/a.rs"]),
            ("crates/y/src/b.rs", vec![]),
            ("crates/y/src/main.rs", vec![]),
            ("loose/z.rs", vec![]),
        ]
    );
}

#[test]
fn pyi_mts_and_cts_files_are_parsed_and_vue_svelte_razor_files_are_empty() {
    let dir = tempfile::tempdir().unwrap();
    let structure = analyze_sources(
        dir.path(),
        &[
            ("a.pyi", "class A: ...\n"),
            ("b.mts", "export function f() {}\n"),
            ("c.cts", "export class C {}\n"),
            ("d.vue", "<script>export default {}</script>\n"),
            ("e.svelte", "<script>let x = 1;</script>\n"),
            ("f.razor", "@code { }\n"),
        ],
    );

    assert_eq!(symbols(&structure, "a.pyi"), vec![("A", Class)]);
    assert_eq!(symbols(&structure, "b.mts"), vec![("f", Function)]);
    assert_eq!(symbols(&structure, "c.cts"), vec![("C", Class)]);
    for path in ["d.vue", "e.svelte", "f.razor"] {
        assert_eq!(symbols(&structure, path), vec![], "{path}");
    }
}

#[test]
fn mjs_and_cjs_specifiers_resolve_to_mts_and_cts_sources() {
    let dir = tempfile::tempdir().unwrap();
    let structure = analyze_sources(
        dir.path(),
        &[
            ("a.mts", "import './b.mjs';\nimport './c.cjs';\n"),
            ("b.mts", "export const b = 1;\n"),
            ("c.cts", "export const c = 1;\n"),
        ],
    );

    assert_eq!(
        edges(&structure),
        vec![
            ("a.mts", vec!["b.mts", "c.cts"]),
            ("b.mts", vec![]),
            ("c.cts", vec![])
        ]
    );
}

#[test]
fn python_imports_resolve_to_stubs_when_no_source_file_exists() {
    let dir = tempfile::tempdir().unwrap();
    let structure = analyze_sources(
        dir.path(),
        &[
            (
                "main.py",
                "from pkg import stub\nimport other\nimport both\n",
            ),
            ("pkg/__init__.pyi", ""),
            ("pkg/stub.pyi", "class S: ...\n"),
            ("other.pyi", "class O: ...\n"),
            ("both.py", "x = 1\n"),
            ("both.pyi", "x: int\n"),
        ],
    );

    let main = edges(&structure)
        .into_iter()
        .find(|(path, _)| *path == "main.py");
    assert_eq!(
        main,
        Some((
            "main.py",
            vec!["both.py", "other.pyi", "pkg/__init__.pyi", "pkg/stub.pyi"]
        ))
    );
}
