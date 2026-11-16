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
