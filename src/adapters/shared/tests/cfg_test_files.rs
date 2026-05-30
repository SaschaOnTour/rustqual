use crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths;

#[test]
fn cfg_test_propagates_transitively_through_mod_chain() {
    // Reproduces the bug where a test-file's own sub-module is not
    // recognised as cfg-test.
    //
    //   src/parent.rs   has `#[cfg(test)] mod tests;`         → cfg-test
    //   src/parent/tests/mod.rs     has `mod golden;`         → should be cfg-test
    //   src/parent/tests/golden.rs  has `fn helper() {}`      → should be cfg-test
    //
    // Previously only tests/mod.rs was tagged cfg-test; golden.rs was
    // treated as production code and its helper was reported as
    // test-only dead code even though it lives in a test-only file.
    let parent_code = r#"
        #[cfg(test)]
        mod tests;
    "#;
    let tests_mod_code = r#"
        mod golden;
    "#;
    let golden_code = r#"
        pub fn helper() -> u32 { 42 }
    "#;
    let parsed = vec![
        (
            "src/parent.rs".to_string(),
            parent_code.to_string(),
            syn::parse_file(parent_code).unwrap(),
        ),
        (
            "src/parent/tests/mod.rs".to_string(),
            tests_mod_code.to_string(),
            syn::parse_file(tests_mod_code).unwrap(),
        ),
        (
            "src/parent/tests/golden.rs".to_string(),
            golden_code.to_string(),
            syn::parse_file(golden_code).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("src/parent/tests/mod.rs"),
        "direct cfg-test mod target not detected: {result:?}"
    );
    assert!(
        result.contains("src/parent/tests/golden.rs"),
        "sub-module of cfg-test file must propagate cfg-test status: {result:?}"
    );
}

#[test]
fn cfg_test_propagation_does_not_tag_unrelated_files() {
    // Negative case: a regular `mod foo;` in a production file must
    // not become cfg-test just because the fix now propagates through
    // cfg-test parents.
    let prod_code = r#"
        mod helpers;
    "#;
    let helpers_code = r#"
        pub fn util() {}
    "#;
    let parsed = vec![
        (
            "src/prod.rs".to_string(),
            prod_code.to_string(),
            syn::parse_file(prod_code).unwrap(),
        ),
        (
            "src/prod/helpers.rs".to_string(),
            helpers_code.to_string(),
            syn::parse_file(helpers_code).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        !result.contains("src/prod/helpers.rs"),
        "production sub-module must not be cfg-test: {result:?}"
    );
}

#[test]
fn per_crate_tests_dir_recognized_as_integration_test() {
    // Cargo compiles `<pkg_root>/tests/*.rs` as integration-test
    // binaries. In a workspace the package root is a sub-directory
    // (`crates/<name>/`), so the integration-test files live at
    // `crates/<name>/tests/*.rs` — not at the workspace-root `tests/`.
    // These must be recognised as test-only just like the top-level
    // layout, otherwise their test-attributed entry points are falsely
    // reported as dead code.
    let code = r#"
        #[tokio::test]
        async fn end_to_end() {}
    "#;
    let lib = "pub fn run() {}";
    let parsed = vec![
        (
            "crates/sv-utility-retry/src/lib.rs".to_string(),
            lib.to_string(),
            syn::parse_file(lib).unwrap(),
        ),
        (
            "crates/sv-utility-retry/tests/integration.rs".to_string(),
            code.to_string(),
            syn::parse_file(code).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("crates/sv-utility-retry/tests/integration.rs"),
        "per-crate tests/ integration file must be cfg-test: {result:?}"
    );
}

#[test]
fn workspace_root_tests_dir_recognized_as_integration_test() {
    // The single-crate layout: integration tests at the analysis-root
    // `tests/` directory, with the crate's own `src/` present so the
    // root ("") qualifies as a package root.
    let code = "#[test] fn it_works() {}";
    let lib = "pub fn run() {}";
    let parsed = vec![
        (
            "src/lib.rs".to_string(),
            lib.to_string(),
            syn::parse_file(lib).unwrap(),
        ),
        (
            "tests/integration.rs".to_string(),
            code.to_string(),
            syn::parse_file(code).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("tests/integration.rs"),
        "analysis-root tests/ integration file must be cfg-test: {result:?}"
    );
}

#[test]
fn tests_dir_only_classified_at_a_real_package_root() {
    // Review: a `tests/` directory counts as Cargo integration tests only
    // when its parent is a real package root — i.e. that directory also
    // holds a `src/` tree in the parsed file set. A `tests/` directory
    // anywhere else (fixtures, tooling, data) is production code and must
    // NOT enter the shared cfg-test set, or findings get hidden across
    // IOSP/SRP/architecture/DRY/TQ.
    let crate_lib = "pub fn run() {}";
    let it = "#[test] fn it() {}";
    let fixture = "pub fn support() {}";
    let tool = "pub fn helper() {}";
    let parsed = vec![
        (
            "crates/x/src/lib.rs".to_string(),
            crate_lib.to_string(),
            syn::parse_file(crate_lib).unwrap(),
        ),
        (
            "crates/x/tests/it.rs".to_string(),
            it.to_string(),
            syn::parse_file(it).unwrap(),
        ),
        (
            "fixtures/tests/support.rs".to_string(),
            fixture.to_string(),
            syn::parse_file(fixture).unwrap(),
        ),
        (
            "tools/shared/tests/helpers.rs".to_string(),
            tool.to_string(),
            syn::parse_file(tool).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("crates/x/tests/it.rs"),
        "tests/ at a real package root (has sibling src/) must be cfg-test: {result:?}"
    );
    assert!(
        !result.contains("fixtures/tests/support.rs"),
        "tests/ with no sibling src/ must NOT be cfg-test: {result:?}"
    );
    assert!(
        !result.contains("tools/shared/tests/helpers.rs"),
        "nested non-package-root tests/ must NOT be cfg-test: {result:?}"
    );
}

#[test]
fn package_nested_under_a_tests_dir_recognizes_its_own_tests() {
    // Edge case: a real package whose path contains an earlier `tests`
    // segment (`fixtures/tests/retry/`). Its OWN integration tests live at
    // `fixtures/tests/retry/tests/…`; the owner of *that* `tests/` is the
    // package root, even though an outer `tests` segment appears first.
    // The outer `fixtures/tests/other.rs` (no package root there) must
    // still NOT be classified.
    let lib = "pub fn run() {}";
    let it = "#[test] fn it() {}";
    let outer = "pub fn outer() {}";
    let parsed = vec![
        (
            "fixtures/tests/retry/src/lib.rs".to_string(),
            lib.to_string(),
            syn::parse_file(lib).unwrap(),
        ),
        (
            "fixtures/tests/retry/tests/integration.rs".to_string(),
            it.to_string(),
            syn::parse_file(it).unwrap(),
        ),
        (
            "fixtures/tests/other.rs".to_string(),
            outer.to_string(),
            syn::parse_file(outer).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("fixtures/tests/retry/tests/integration.rs"),
        "the package's own tests/ (owner = package root) must be cfg-test: {result:?}"
    );
    assert!(
        !result.contains("fixtures/tests/other.rs"),
        "an outer `tests` dir that is not a package root must NOT be cfg-test: {result:?}"
    );
}

#[test]
fn tests_dir_next_to_src_without_crate_root_file_not_classified() {
    // The package-root signal is Cargo's crate-root file (`src/lib.rs` or
    // `src/main.rs`), derived from the parsed set — NOT the mere presence
    // of a `src/` segment. A directory with a `src/` subtree but no
    // crate-root file is not a package, so its sibling `tests/` is
    // production code, not integration tests. Guards the case of a
    // deeply nested coincidental `src/`+`tests/` pair.
    let internal = "pub fn data() {}"; // under src/, but not a crate root
    let support = "pub fn support() {}";
    let parsed = vec![
        (
            "vendor/widget/src/internal.rs".to_string(),
            internal.to_string(),
            syn::parse_file(internal).unwrap(),
        ),
        (
            "vendor/widget/tests/support.rs".to_string(),
            support.to_string(),
            syn::parse_file(support).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        !result.contains("vendor/widget/tests/support.rs"),
        "tests/ next to a src/ without a crate-root file must NOT be cfg-test: {result:?}"
    );
}

#[test]
fn tests_dir_next_to_crate_root_file_classified() {
    // The flip side: a `src/lib.rs` (or `src/main.rs`) marks a real
    // package root, so its sibling `tests/` is integration tests.
    let lib = "pub fn run() {}";
    let it = "#[test] fn it() {}";
    let parsed = vec![
        (
            "vendor/widget/src/lib.rs".to_string(),
            lib.to_string(),
            syn::parse_file(lib).unwrap(),
        ),
        (
            "vendor/widget/tests/support.rs".to_string(),
            it.to_string(),
            syn::parse_file(it).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("vendor/widget/tests/support.rs"),
        "tests/ next to a crate-root file must be cfg-test: {result:?}"
    );
}

#[test]
fn tests_dir_next_to_bin_only_crate_classified() {
    // A binary-only package (autobins in `src/bin/`, no `lib.rs`/`main.rs`)
    // is a real crate root, so its sibling `tests/` are integration tests.
    let bin = "fn main() {}";
    let it = "#[test] fn it() {}";
    let parsed = vec![
        (
            "crates/cli/src/bin/tool.rs".to_string(),
            bin.to_string(),
            syn::parse_file(bin).unwrap(),
        ),
        (
            "crates/cli/tests/it.rs".to_string(),
            it.to_string(),
            syn::parse_file(it).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("crates/cli/tests/it.rs"),
        "tests/ next to a src/bin/ autobinary crate must be cfg-test: {result:?}"
    );
}

#[test]
fn is_integration_test_path_matches_package_root_tests_only() {
    use crate::adapters::shared::cfg_test_files::is_integration_test_path;
    use std::collections::HashSet;
    // Package roots derived from the file set: the analysis-root crate
    // (""), a workspace member, and a member whose name contains "src".
    let roots: HashSet<String> = ["", "crates/x", "crates/my-src-tool"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // `tests/` directly under a package root → integration test.
    assert!(is_integration_test_path("tests/it.rs", &roots));
    assert!(is_integration_test_path("crates/x/tests/it.rs", &roots));
    // A member whose name contains "src" is matched by its real root.
    assert!(is_integration_test_path(
        "crates/my-src-tool/tests/it.rs",
        &roots
    ));
    // `tests/` whose owning directory is not a package root → not an
    // integration test (fixtures, tooling, nested unit-test submodules).
    assert!(!is_integration_test_path(
        "fixtures/tests/support.rs",
        &roots
    ));
    assert!(!is_integration_test_path(
        "tools/shared/tests/helpers.rs",
        &roots
    ));
    assert!(!is_integration_test_path("src/foo/tests/bar.rs", &roots));
    assert!(!is_integration_test_path(
        "crates/x/src/foo/tests/bar.rs",
        &roots
    ));
    // No `tests/` segment at all.
    assert!(!is_integration_test_path("src/lib.rs", &roots));
}

#[test]
fn nested_src_tests_dir_not_blanket_classified_as_integration() {
    // Regression guard for the review finding: `contains("/tests/")` was
    // too broad and classified ANY nested `tests` directory as test-only.
    // A plain production file under `src/**/tests/` that is NOT reached
    // via `#[cfg(test)] mod` must not be auto-classified — otherwise real
    // findings get hidden across dimensions.
    let code = "pub fn connect() {}";
    let parsed = vec![(
        "src/database/tests/connection.rs".to_string(),
        code.to_string(),
        syn::parse_file(code).unwrap(),
    )];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        !result.contains("src/database/tests/connection.rs"),
        "production file under src/**/tests/ must not be auto cfg-test: {result:?}"
    );
}

#[test]
fn src_tests_dir_reached_via_cfg_test_mod_still_classified() {
    // The flip side: when a `src/**/tests/` file IS reached through a
    // `#[cfg(test)] mod tests;` chain it must still be cfg-test — that
    // path comes from the mod-chain detector, not the integration-path
    // heuristic, so tightening the heuristic must not regress it.
    let parent_code = r#"
        #[cfg(test)]
        mod tests;
    "#;
    let child_code = "pub fn helper() -> u32 { 42 }";
    let parsed = vec![
        (
            "src/database.rs".to_string(),
            parent_code.to_string(),
            syn::parse_file(parent_code).unwrap(),
        ),
        (
            "src/database/tests.rs".to_string(),
            child_code.to_string(),
            syn::parse_file(child_code).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("src/database/tests.rs"),
        "src test file reached via #[cfg(test)] mod must still be cfg-test: {result:?}"
    );
}

#[test]
fn collect_cfg_test_file_paths_basic() {
    let parent_code = r#"
        #[cfg(test)]
        mod helpers;
    "#;
    let child_code = "pub fn h() {}";
    let parent_ast = syn::parse_file(parent_code).unwrap();
    let child_ast = syn::parse_file(child_code).unwrap();
    let parsed = vec![
        (
            "src/lib.rs".to_string(),
            parent_code.to_string(),
            parent_ast,
        ),
        (
            "src/helpers.rs".to_string(),
            child_code.to_string(),
            child_ast,
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("src/helpers.rs"),
        "Should detect src/helpers.rs as cfg-test file"
    );
}

// ── Companion-file detection ─────────────────────────────────────

#[test]
fn inner_cfg_test_file_attribute_marks_file_as_cfg_test() {
    // `#![cfg(test)]` as an inner attribute at the top of a file is
    // the Rust-level "this whole file is test-only" signal. rustqual
    // must recognize it on its own — without requiring a parent
    // `#[cfg(test)] mod foo;` elsewhere.
    let code = r#"
        #![cfg(test)]
        pub fn helper() -> u32 { 42 }
    "#;
    let parsed = vec![(
        "src/foo_tests.rs".to_string(),
        code.to_string(),
        syn::parse_file(code).unwrap(),
    )];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("src/foo_tests.rs"),
        "inner `#![cfg(test)]` must tag the file as cfg-test: {result:?}"
    );
}

#[test]
fn path_attribute_on_cfg_test_mod_redirects_to_target_file() {
    // Companion-file pattern common in test-co-location:
    //
    //   // src/foo.rs (production):
    //   #[cfg(test)]
    //   #[path = "foo_tests.rs"]
    //   mod tests;
    //
    // The convention-based resolver would look for src/foo/tests.rs
    // or src/foo/tests/mod.rs — neither exists. The `#[path]` points
    // to the sibling file and must be honored.
    let parent_code = r#"
        #[cfg(test)]
        #[path = "foo_tests.rs"]
        mod tests;
    "#;
    let companion_code = r#"
        pub fn helper() -> u32 { 42 }
    "#;
    let parsed = vec![
        (
            "src/foo.rs".to_string(),
            parent_code.to_string(),
            syn::parse_file(parent_code).unwrap(),
        ),
        (
            "src/foo_tests.rs".to_string(),
            companion_code.to_string(),
            syn::parse_file(companion_code).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("src/foo_tests.rs"),
        "`#[path]` on a cfg-test mod must redirect to the target file: {result:?}"
    );
}

#[test]
fn path_attribute_resolves_relative_to_parent_dir() {
    // `#[path]` is relative to the *directory containing the parent
    // file* — mimic the Rust compiler semantics.
    //
    //   src/ingest/code/rust.rs:
    //     #[cfg(test)] #[path = "rust_tests.rs"] mod tests;
    //
    //   src/ingest/code/rust_tests.rs  ← target
    let parent_code = r#"
        #[cfg(test)]
        #[path = "rust_tests.rs"]
        mod tests;
    "#;
    let companion_code = r#"
        pub fn helper() -> u32 { 42 }
    "#;
    let parsed = vec![
        (
            "src/ingest/code/rust.rs".to_string(),
            parent_code.to_string(),
            syn::parse_file(parent_code).unwrap(),
        ),
        (
            "src/ingest/code/rust_tests.rs".to_string(),
            companion_code.to_string(),
            syn::parse_file(companion_code).unwrap(),
        ),
    ];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.contains("src/ingest/code/rust_tests.rs"),
        "relative `#[path]` must resolve against the parent file's directory: {result:?}"
    );
}
