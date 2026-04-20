use crate::adapters::analyzers::dry::fragments::*;
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
const TEST_MIN_STATEMENTS: usize = 3;

fn low_threshold_config() -> DuplicatesConfig {
    DuplicatesConfig {
        min_tokens: TEST_MIN_TOKENS,
        min_lines: 1,
        min_statements: TEST_MIN_STATEMENTS,
        ..DuplicatesConfig::default()
    }
}

#[test]
fn test_detect_fragments_empty() {
    let parsed = parse("");
    let config = low_threshold_config();
    let groups = detect_fragments(&parsed, &config);
    assert!(groups.is_empty());
}

#[test]
fn test_detect_fragments_no_match() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            r#"
            fn foo() {
                let x = 1;
                let y = x + 2;
                let z = y * x;
            }
        "#,
        ),
        (
            "b.rs",
            r#"
            fn bar() {
                let a = "hello";
                let b = a.len();
                if b > 0 { return; }
            }
        "#,
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_fragments(&parsed, &config);
    assert!(groups.is_empty(), "Different structures should not match");
}

#[test]
fn test_detect_fragments_matching_statements() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            r#"
            fn foo() {
                let x = 1;
                let y = x + 2;
                let z = y * x;
            }
        "#,
        ),
        (
            "b.rs",
            r#"
            fn bar() {
                let a = 1;
                let b = a + 2;
                let c = b * a;
            }
        "#,
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_fragments(&parsed, &config);
    assert!(!groups.is_empty(), "Should detect matching fragment");
    assert_eq!(groups[0].entries.len(), 2);
}

#[test]
fn test_detect_fragments_cross_file() {
    let parsed = parse_multi(&[
        (
            "module_a.rs",
            r#"
            fn process_a() {
                let x = 1;
                let y = x + 2;
                let z = y * x;
                let w = z + 1;
            }
        "#,
        ),
        (
            "module_b.rs",
            r#"
            fn process_b() {
                let a = 1;
                let b = a + 2;
                let c = b * a;
                let d = c + 1;
            }
        "#,
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_fragments(&parsed, &config);
    assert!(!groups.is_empty());
    if let Some(g) = groups.first() {
        let files: std::collections::HashSet<&str> =
            g.entries.iter().map(|e| e.file.as_str()).collect();
        assert!(
            files.len() >= 2,
            "Fragment entries should come from different files"
        );
    }
}

#[test]
fn test_detect_fragments_same_function_excluded() {
    let parsed = parse(
        r#"
        fn foo() {
            let x = 1;
            let y = x + 2;
            let z = y * x;
            let a = 1;
            let b = a + 2;
            let c = b * a;
        }
    "#,
    );
    let config = low_threshold_config();
    let groups = detect_fragments(&parsed, &config);
    assert!(
        groups.is_empty(),
        "Same-function duplicates should be excluded"
    );
}

#[test]
fn test_detect_fragments_merges_adjacent() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            r#"
            fn foo() {
                let x = 1;
                let y = x + 2;
                let z = y * x;
                let w = z + 1;
            }
        "#,
        ),
        (
            "b.rs",
            r#"
            fn bar() {
                let a = 1;
                let b = a + 2;
                let c = b * a;
                let d = c + 1;
            }
        "#,
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_fragments(&parsed, &config);
    // Windows [0,1,2] and [1,2,3] both match → merge into 4-statement fragment
    if !groups.is_empty() {
        assert!(
            groups.iter().any(|g| g.statement_count >= 4),
            "Adjacent windows should merge: got counts {:?}",
            groups.iter().map(|g| g.statement_count).collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_detect_fragments_too_few_statements() {
    let parsed = parse_multi(&[
        ("a.rs", "fn foo() { let x = 1; let y = 2; }"),
        ("b.rs", "fn bar() { let a = 1; let b = 2; }"),
    ]);
    let mut config = low_threshold_config();
    config.min_statements = 3;
    let groups = detect_fragments(&parsed, &config);
    assert!(
        groups.is_empty(),
        "Functions with <min_statements should produce no fragments"
    );
}

#[test]
fn test_detect_fragments_below_min_tokens() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            r#"
            fn foo() {
                let x = 1;
                let y = 2;
                let z = 3;
            }
        "#,
        ),
        (
            "b.rs",
            r#"
            fn bar() {
                let a = 1;
                let b = 2;
                let c = 3;
            }
        "#,
        ),
    ]);
    let mut config = low_threshold_config();
    config.min_tokens = 100;
    let groups = detect_fragments(&parsed, &config);
    assert!(
        groups.is_empty(),
        "Windows below min_tokens should be excluded"
    );
}

#[test]
fn test_detect_fragments_test_excluded() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            r#"
            fn prod() {
                let x = 1;
                let y = x + 2;
                let z = y * x;
            }
        "#,
        ),
        (
            "b.rs",
            r#"
            #[cfg(test)]
            mod tests {
                fn test_helper() {
                    let a = 1;
                    let b = a + 2;
                    let c = b * a;
                }
            }
        "#,
        ),
    ]);
    let mut config = low_threshold_config();
    config.ignore_tests = true;
    let groups = detect_fragments(&parsed, &config);
    assert!(
        groups.is_empty(),
        "Test functions should be excluded when ignore_tests=true"
    );
}

#[test]
fn test_detect_fragments_test_included() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            r#"
            fn prod() {
                let x = 1;
                let y = x + 2;
                let z = y * x;
            }
        "#,
        ),
        (
            "b.rs",
            r#"
            #[cfg(test)]
            mod tests {
                fn test_helper() {
                    let a = 1;
                    let b = a + 2;
                    let c = b * a;
                }
            }
        "#,
        ),
    ]);
    let mut config = low_threshold_config();
    config.ignore_tests = false;
    let groups = detect_fragments(&parsed, &config);
    assert!(
        !groups.is_empty(),
        "Test functions should be included when ignore_tests=false"
    );
}

#[test]
fn test_detect_fragments_entry_has_lines() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            r#"
            fn foo() {
                let x = 1;
                let y = x + 2;
                let z = y * x;
            }
        "#,
        ),
        (
            "b.rs",
            r#"
            fn bar() {
                let a = 1;
                let b = a + 2;
                let c = b * a;
            }
        "#,
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_fragments(&parsed, &config);
    if !groups.is_empty() {
        for entry in &groups[0].entries {
            assert!(entry.start_line > 0, "start_line should be > 0");
            assert!(entry.end_line >= entry.start_line, "end_line >= start_line");
        }
    }
}

#[test]
fn test_extract_matching_pairs_empty() {
    let pairs = extract_matching_pairs(&[]);
    assert!(pairs.is_empty());
}

#[test]
fn test_merge_into_fragments_empty() {
    let fn_infos: Vec<FnInfo> = vec![];
    let groups = merge_into_fragments(vec![], &fn_infos, 3);
    assert!(groups.is_empty());
}

#[test]
fn test_fragment_group_statement_count() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            r#"
            fn foo() {
                let x = 1;
                let y = x + 2;
                let z = y * x;
            }
        "#,
        ),
        (
            "b.rs",
            r#"
            fn bar() {
                let a = 1;
                let b = a + 2;
                let c = b * a;
            }
        "#,
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_fragments(&parsed, &config);
    for group in &groups {
        assert!(
            group.statement_count >= 3,
            "Fragment must have at least min_statements"
        );
    }
}

#[test]
fn test_detect_fragments_impl_method() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            r#"
            struct Foo;
            impl Foo {
                fn method(&self) {
                    let x = 1;
                    let y = x + 2;
                    let z = y * x;
                }
            }
        "#,
        ),
        (
            "b.rs",
            r#"
            struct Bar;
            impl Bar {
                fn method(&self) {
                    let a = 1;
                    let b = a + 2;
                    let c = b * a;
                }
            }
        "#,
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_fragments(&parsed, &config);
    assert!(
        !groups.is_empty(),
        "Should detect fragments in impl methods"
    );
}

#[test]
fn test_detect_fragments_three_way() {
    let parsed = parse_multi(&[
        (
            "a.rs",
            r#"
            fn func_a() {
                let x = 1;
                let y = x + 2;
                let z = y * x;
            }
        "#,
        ),
        (
            "b.rs",
            r#"
            fn func_b() {
                let a = 1;
                let b = a + 2;
                let c = b * a;
            }
        "#,
        ),
        (
            "c.rs",
            r#"
            fn func_c() {
                let p = 1;
                let q = p + 2;
                let r = q * p;
            }
        "#,
        ),
    ]);
    let config = low_threshold_config();
    let groups = detect_fragments(&parsed, &config);
    // With 3 matching functions, we get pairs: (a,b), (a,c), (b,c)
    assert!(
        groups.len() >= 3,
        "Three matching functions should produce at least 3 pair groups"
    );
}
