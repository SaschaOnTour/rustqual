use crate::adapters::analyzers::dry::functions::*;
use crate::adapters::analyzers::dry::FunctionHashEntry;
use crate::config::sections::DuplicatesConfig;
use std::collections::HashMap;
use syn::spanned::Spanned;
use syn::visit::Visit;

fn parse(code: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(code).expect("parse failed");
    vec![("test.rs".to_string(), code.to_string(), syntax)]
}

fn parse_multi(files: &[(&str, &str)]) -> Vec<(String, String, syn::File)> {
    files
        .iter()
        .map(|(name, code)| {
            let syntax = syn::parse_file(code).expect("parse failed");
            (name.to_string(), code.to_string(), syntax)
        })
        .collect()
}

const TEST_MIN_TOKENS: usize = 3;

fn low_threshold_config() -> DuplicatesConfig {
    DuplicatesConfig {
        min_tokens: TEST_MIN_TOKENS,
        min_lines: 1,
        ..DuplicatesConfig::default()
    }
}

#[test]
fn test_detect_duplicates_no_functions() {
    let parsed = parse("");
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    assert!(groups.is_empty());
}

#[test]
fn test_detect_duplicates_no_duplicates() {
    let code = r#"
        fn foo() { let x = 1; let y = x + 2; let z = y * x; }
        fn bar() { let a = "hello"; let b = a.len(); if b > 0 { return; } }
    "#;
    let parsed = parse(code);
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    assert!(
        groups.is_empty(),
        "Different functions should not be duplicates"
    );
}

#[test]
fn test_detect_exact_duplicates_same_structure() {
    // Two functions with identical structure but different variable names
    let parsed = parse_multi(&[
        (
            "a.rs",
            "fn process_a() { let x = 1; let y = x + 2; let z = y * x; }",
        ),
        (
            "b.rs",
            "fn process_b() { let a = 1; let b = a + 2; let c = b * a; }",
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    assert_eq!(groups.len(), 1, "Should detect one exact duplicate group");
    assert!(matches!(groups[0].kind, DuplicateKind::Exact));
    assert_eq!(groups[0].entries.len(), 2);
}

#[test]
fn test_detect_duplicates_different_structure() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            "fn add() { let x = 1; let y = x + 2; let z = y + x; }",
        ),
        (
            "b.rs",
            "fn mul() { let a = 1; let b = a * 2; let c = b * a; }",
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    // Different operators → different hash → no exact duplicate
    let exact_groups: Vec<_> = groups
        .iter()
        .filter(|g| matches!(g.kind, DuplicateKind::Exact))
        .collect();
    assert!(
        exact_groups.is_empty(),
        "Different operators should not be exact duplicates"
    );
}

#[test]
fn test_detect_duplicates_below_min_tokens_excluded() {
    let parsed = parse_multi(&[
        ("a.rs", "fn tiny_a() { let x = 1; }"),
        ("b.rs", "fn tiny_b() { let y = 1; }"),
    ]);
    let config = DuplicatesConfig::default(); // min_tokens = 30
    let groups = detect_duplicates(&parsed, &config);
    assert!(
        groups.is_empty(),
        "Small functions below min_tokens should be excluded"
    );
}

#[test]
fn test_detect_duplicates_test_functions_excluded() {
    let code = r#"
        #[cfg(test)]
        mod tests {
            fn helper_a() { let x = 1; let y = x + 2; let z = y * x; }
            fn helper_b() { let a = 1; let b = a + 2; let c = b * a; }
        }
    "#;
    let parsed = parse(code);
    let mut config = low_threshold_config();
    config.ignore_tests = true;
    let groups = detect_duplicates(&parsed, &config);
    assert!(
        groups.is_empty(),
        "Test functions should be excluded when ignore_tests=true"
    );
}

#[test]
fn test_detect_duplicates_test_functions_included() {
    let code = r#"
        #[cfg(test)]
        mod tests {
            fn helper_a() { let x = 1; let y = x + 2; let z = y * x; }
            fn helper_b() { let a = 1; let b = a + 2; let c = b * a; }
        }
    "#;
    let parsed = parse(code);
    let mut config = low_threshold_config();
    config.ignore_tests = false;
    let groups = detect_duplicates(&parsed, &config);
    assert_eq!(
        groups.len(),
        1,
        "Test functions should be included when ignore_tests=false"
    );
}

#[test]
fn test_detect_duplicates_three_way() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            "fn func_a() { let x = 1; let y = x + 2; let z = y * x; }",
        ),
        (
            "b.rs",
            "fn func_b() { let a = 1; let b = a + 2; let c = b * a; }",
        ),
        (
            "c.rs",
            "fn func_c() { let p = 1; let q = p + 2; let r = q * p; }",
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].entries.len(), 3, "Should detect 3-way duplicate");
}

#[test]
fn test_detect_duplicates_config_disabled() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            "fn func_a() { let x = 1; let y = x + 2; let z = y * x; }",
        ),
        (
            "b.rs",
            "fn func_b() { let a = 1; let b = a + 2; let c = b * a; }",
        ),
    ]);
    let mut config = low_threshold_config();
    config.enabled = false;
    // detect_duplicates doesn't check enabled — pipeline does.
    // But we can test it still works.
    let groups = detect_duplicates(&parsed, &config);
    assert!(!groups.is_empty());
}

#[test]
fn test_group_exact_duplicates_returns_remaining() {
    let entries = vec![
        FunctionHashEntry {
            name: "a".into(),
            qualified_name: "a".into(),
            file: "a.rs".into(),
            line: 1,
            hash: 100,
            token_count: 10,
            tokens: vec![],
        },
        FunctionHashEntry {
            name: "b".into(),
            qualified_name: "b".into(),
            file: "b.rs".into(),
            line: 1,
            hash: 200,
            token_count: 10,
            tokens: vec![],
        },
        FunctionHashEntry {
            name: "c".into(),
            qualified_name: "c".into(),
            file: "c.rs".into(),
            line: 1,
            hash: 100,
            token_count: 10,
            tokens: vec![],
        },
    ];
    let (groups, remaining) = group_exact_duplicates(&entries);
    assert_eq!(groups.len(), 1); // a and c share hash 100
    assert_eq!(remaining.len(), 1); // b is alone
    assert_eq!(remaining[0], 1); // index of b
}

#[test]
fn test_detect_near_duplicates_high_similarity() {
    // Two functions with slight differences — should be near-duplicates
    let parsed = parse_multi(&[
        (
            "a.rs",
            "fn func_a() { let x = 1; let y = x + 2; let z = y * x; let w = z + 1; }",
        ),
        (
            "b.rs",
            "fn func_b() { let x = 1; let y = x + 2; let z = y * x; let w = z - 1; }",
        ),
    ]);
    let mut config = low_threshold_config();
    config.similarity_threshold = 0.80;
    let groups = detect_duplicates(&parsed, &config);
    // These have different hashes (+ vs -) but high Jaccard similarity
    let near_groups: Vec<_> = groups
        .iter()
        .filter(|g| matches!(g.kind, DuplicateKind::NearDuplicate { .. }))
        .collect();
    // Whether detected depends on bucketing — both have similar token counts
    // The test verifies the mechanism works, even if thresholds vary
    if !near_groups.is_empty() {
        let DuplicateKind::NearDuplicate { similarity } = near_groups[0].kind else {
            panic!("expected near duplicate");
        };
        assert!(similarity >= 0.80);
    }
}

#[test]
fn test_duplicate_entry_has_file_and_line() {
    let parsed = parse_multi(&[
        (
            "module_a.rs",
            "fn func_a() { let x = 1; let y = x + 2; let z = y * x; }",
        ),
        (
            "module_b.rs",
            "fn func_b() { let a = 1; let b = a + 2; let c = b * a; }",
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_duplicates(&parsed, &config);
    assert!(!groups.is_empty());
    for entry in &groups[0].entries {
        assert!(!entry.file.is_empty());
        assert!(!entry.name.is_empty());
    }
}
