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
