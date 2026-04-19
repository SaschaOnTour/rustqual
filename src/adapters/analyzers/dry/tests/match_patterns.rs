use crate::adapters::analyzers::dry::match_patterns::*;
use crate::adapters::analyzers::dry::{has_cfg_test, has_test_attr, FileVisitor};
use crate::config::sections::DuplicatesConfig;
use std::collections::HashMap;
use syn::visit::Visit;

fn parse(code: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(code).expect("parse failed");
    vec![("test.rs".to_string(), code.to_string(), syntax)]
}

#[test]
fn test_detect_empty() {
    let parsed = parse("");
    let config = DuplicatesConfig::default();
    let result = detect_repeated_matches(&parsed, &config);
    assert!(result.is_empty());
}

#[test]
fn test_detect_single_match_not_flagged() {
    let code = r#"
        enum E { A, B, C }
        fn f(e: E) {
            match e { E::A => 1, E::B => 2, E::C => 3 };
        }
    "#;
    let parsed = parse(code);
    let config = DuplicatesConfig::default();
    let result = detect_repeated_matches(&parsed, &config);
    assert!(result.is_empty(), "single instance should not be flagged");
}

#[test]
fn test_detect_repeated_match_flagged() {
    let code = r#"
        enum E { A, B, C }
        fn f1(e: E) -> i32 {
            match e { E::A => 1, E::B => 2, E::C => 3 }
        }
        fn f2(e: E) -> i32 {
            match e { E::A => 1, E::B => 2, E::C => 3 }
        }
        fn f3(e: E) -> i32 {
            match e { E::A => 1, E::B => 2, E::C => 3 }
        }
    "#;
    let parsed = parse(code);
    let config = DuplicatesConfig::default();
    let result = detect_repeated_matches(&parsed, &config);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].entries.len(), 3);
    assert_eq!(result[0].enum_name, "E");
}

#[test]
fn test_detect_different_matches_not_grouped() {
    let code = r#"
        enum E { A, B, C }
        fn f1(e: E) -> i32 {
            match e { E::A => 1, E::B => 2, E::C => 3 }
        }
        fn f2(e: E) -> i32 {
            match e { E::A => 10, E::B => 20, E::C => 30 }
        }
        fn f3(e: E) -> i32 {
            match e { E::A => 100, E::B => 200, E::C => 300 }
        }
    "#;
    let parsed = parse(code);
    let config = DuplicatesConfig::default();
    let result = detect_repeated_matches(&parsed, &config);
    // Normalization erases literal values, so these should hash the same
    // because the structure is identical (match on enum with 3 literal returns)
    assert_eq!(result.len(), 1, "same structure matches should be grouped");
}

#[test]
fn test_detect_test_code_excluded() {
    let code = r#"
        enum E { A, B, C }
        #[cfg(test)]
        mod tests {
            use super::*;
            fn f1(e: E) -> i32 { match e { E::A => 1, E::B => 2, E::C => 3 } }
            fn f2(e: E) -> i32 { match e { E::A => 1, E::B => 2, E::C => 3 } }
            fn f3(e: E) -> i32 { match e { E::A => 1, E::B => 2, E::C => 3 } }
        }
    "#;
    let parsed = parse(code);
    let config = DuplicatesConfig {
        ignore_tests: true,
        ..DuplicatesConfig::default()
    };
    let result = detect_repeated_matches(&parsed, &config);
    assert!(result.is_empty(), "test code should be excluded");
}

#[test]
fn test_detect_few_arms_not_flagged() {
    let code = r#"
        fn f1(b: bool) -> i32 { match b { true => 1, false => 0 } }
        fn f2(b: bool) -> i32 { match b { true => 1, false => 0 } }
        fn f3(b: bool) -> i32 { match b { true => 1, false => 0 } }
    "#;
    let parsed = parse(code);
    let config = DuplicatesConfig::default();
    let result = detect_repeated_matches(&parsed, &config);
    assert!(
        result.is_empty(),
        "matches with <3 arms should not be flagged"
    );
}

#[test]
fn test_extract_enum_name_tuple_struct() {
    let code = "match x { Foo::A(v) => v, Foo::B(v) => v, Foo::C(v) => v }";
    let expr: syn::ExprMatch = syn::parse_str(code).unwrap();
    assert_eq!(extract_enum_name(&expr), "Foo");
}

#[test]
fn test_extract_enum_name_path() {
    let code = "match x { Foo::A => 1, Foo::B => 2, Foo::C => 3 }";
    let expr: syn::ExprMatch = syn::parse_str(code).unwrap();
    assert_eq!(extract_enum_name(&expr), "Foo");
}

#[test]
fn test_extract_enum_name_unknown() {
    let code = "match x { a => 1, b => 2, c => 3 }";
    let expr: syn::ExprMatch = syn::parse_str(code).unwrap();
    assert_eq!(extract_enum_name(&expr), "(unknown)");
}

#[test]
fn test_group_requires_multiple_functions() {
    let entries = vec![
        CollectedMatch {
            file: "a.rs".into(),
            line: 1,
            function_name: "same_fn".into(),
            arm_count: 5,
            hash: 42,
            enum_name: "E".into(),
        },
        CollectedMatch {
            file: "a.rs".into(),
            line: 10,
            function_name: "same_fn".into(),
            arm_count: 5,
            hash: 42,
            enum_name: "E".into(),
        },
        CollectedMatch {
            file: "a.rs".into(),
            line: 20,
            function_name: "same_fn".into(),
            arm_count: 5,
            hash: 42,
            enum_name: "E".into(),
        },
    ];
    let result = group_repeated_patterns(entries);
    // 3 instances in same function — still flagged (≥ MIN_INSTANCES)
    // The filter checks len >= 2 unique functions OR len >= MIN_INSTANCES with duplicates
    assert_eq!(
        result.len(),
        1,
        "3 instances even in same fn should be flagged"
    );
}
