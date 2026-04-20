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
