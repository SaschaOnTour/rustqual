use crate::adapters::analyzers::dry::DeclaredFunction;
use crate::adapters::analyzers::iosp::scope::ProjectScope;
use crate::adapters::analyzers::tq::sut::*;
use crate::adapters::analyzers::tq::{TqWarning, TqWarningKind};
use std::collections::HashSet;
use syn::visit::Visit;

fn make_declared(name: &str, is_test: bool) -> DeclaredFunction {
    DeclaredFunction {
        name: name.to_string(),
        qualified_name: name.to_string(),
        file: "lib.rs".to_string(),
        line: 1,
        is_test,
        is_main: false,
        is_trait_impl: false,
        has_allow_dead_code: false,
        is_api: false,
        is_test_helper: false,
    }
}

fn parse_and_detect(source: &str, declared: &[DeclaredFunction]) -> Vec<TqWarning> {
    let syntax = syn::parse_file(source).expect("test source");
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let scope_source = "fn prod_fn() {} fn helper() {}";
    let scope_syntax = syn::parse_file(scope_source).expect("scope source");
    let scope_refs = vec![("lib.rs", &scope_syntax)];
    let scope = ProjectScope::from_files(&scope_refs);
    let reaches_prod = HashSet::new();
    detect_no_sut_tests(&parsed, &scope, declared, &reaches_prod)
}

#[test]
fn test_calls_prod_function_no_warning() {
    let declared = vec![make_declared("prod_fn", false)];
    let warnings = parse_and_detect(
        r#"
        #[test]
        fn test_it() {
            prod_fn();
            assert!(true);
        }
        "#,
        &declared,
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_calls_only_external_emits_warning() {
    let declared = vec![make_declared("prod_fn", false)];
    let warnings = parse_and_detect(
        r#"
        #[test]
        fn test_it() {
            let x = 42;
        }
        "#,
        &declared,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind, TqWarningKind::NoSut);
}

#[test]
fn test_calls_method_on_scope_no_warning() {
    let declared = vec![make_declared("helper", false)];
    let warnings = parse_and_detect(
        r#"
        #[test]
        fn test_it() {
            helper();
        }
        "#,
        &declared,
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_non_test_function_ignored() {
    let declared = vec![make_declared("prod_fn", false)];
    let warnings = parse_and_detect(
        r#"
        fn not_a_test() {
            let x = 42;
        }
        "#,
        &declared,
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_empty_test_emits_warning() {
    let declared = vec![make_declared("prod_fn", false)];
    let warnings = parse_and_detect(
        r#"
        #[test]
        fn test_empty() {}
        "#,
        &declared,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].function_name, "test_empty");
}

#[test]
fn test_type_constructor_recognized_as_sut() {
    let declared = vec![make_declared("new", false)];
    // Build a scope that includes a type named "MyType"
    let scope_source = "struct MyType {} impl MyType { fn new() -> Self { MyType {} } }";
    let scope_syntax = syn::parse_file(scope_source).expect("scope source");
    let scope_refs = vec![("lib.rs", &scope_syntax)];
    let scope = ProjectScope::from_files(&scope_refs);
    // Test: calling MyType::new() should count as SUT
    let source = r#"
        #[test]
        fn test_constructor() {
            let x = MyType::new();
        }
    "#;
    let syntax = syn::parse_file(source).expect("test source");
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let reaches_prod = HashSet::new();
    let warnings = detect_no_sut_tests(&parsed, &scope, &declared, &reaches_prod);
    assert!(
        warnings.is_empty(),
        "MyType::new() should be recognized as SUT call"
    );
}

#[test]
fn test_static_method_recognized_as_sut() {
    let declared = vec![make_declared("load", false)];
    let scope_source = "struct Config {} impl Config { fn load() -> Self { Config {} } }";
    let scope_syntax = syn::parse_file(scope_source).expect("scope source");
    let scope_refs = vec![("lib.rs", &scope_syntax)];
    let scope = ProjectScope::from_files(&scope_refs);
    let source = r#"
        #[test]
        fn test_load() {
            let c = Config::load();
        }
    "#;
    let syntax = syn::parse_file(source).expect("test source");
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let reaches_prod = HashSet::new();
    let warnings = detect_no_sut_tests(&parsed, &scope, &declared, &reaches_prod);
    assert!(
        warnings.is_empty(),
        "Config::load() should be recognized as SUT call"
    );
}

#[test]
fn test_transitive_sut_via_helper() {
    let declared = vec![make_declared("prod_fn", false)];
    let scope_source = "fn prod_fn() {}";
    let scope_syntax = syn::parse_file(scope_source).expect("scope source");
    let scope_refs = vec![("lib.rs", &scope_syntax)];
    let scope = ProjectScope::from_files(&scope_refs);
    let source = r#"
        #[test]
        fn test_via_helper() {
            my_helper();
        }
    "#;
    let syntax = syn::parse_file(source).expect("test source");
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    // my_helper transitively reaches prod_fn
    let reaches_prod: HashSet<String> = ["my_helper".to_string()].into();
    let warnings = detect_no_sut_tests(&parsed, &scope, &declared, &reaches_prod);
    assert!(
        warnings.is_empty(),
        "my_helper transitively calls prod code"
    );
}
