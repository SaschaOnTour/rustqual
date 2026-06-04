use crate::adapters::analyzers::tq::sut::*;
use crate::adapters::analyzers::tq::{TqWarning, TqWarningKind};
use crate::adapters::shared::declared_function::DeclaredFunction;
use crate::adapters::shared::project_scope::ProjectScope;
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

/// Run SUT detection over `source` against an explicit `scope_source` and
/// `reaches` set. The single arrange shared by every SUT test.
fn detect_in(
    source: &str,
    declared: &[DeclaredFunction],
    scope_source: &str,
    reaches: &[&str],
) -> Vec<TqWarning> {
    let syntax = syn::parse_file(source).expect("test source");
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let scope_syntax = syn::parse_file(scope_source).expect("scope source");
    let scope_refs = vec![("lib.rs", &scope_syntax)];
    let scope = ProjectScope::from_files(&scope_refs);
    let reaches_prod: HashSet<String> = reaches.iter().map(|s| s.to_string()).collect();
    detect_no_sut_tests(&parsed, &scope, declared, &reaches_prod)
}

fn parse_and_detect(source: &str, declared: &[DeclaredFunction]) -> Vec<TqWarning> {
    detect_in(source, declared, "fn prod_fn() {} fn helper() {}", &[])
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
fn test_associated_function_call_recognized_as_sut() {
    // Any associated-function call on an in-scope type (`Type::assoc()`) counts
    // as a SUT call — there's no `new`-specific path, so a constructor and a
    // plain static method exercise the same recognition.
    let declared = vec![make_declared("new", false)];
    let warnings = detect_in(
        r#"
        #[test]
        fn test_constructor() {
            let x = MyType::new();
        }
    "#,
        &declared,
        "struct MyType {} impl MyType { fn new() -> Self { MyType {} } }",
        &[],
    );
    assert!(
        warnings.is_empty(),
        "MyType::new() should be recognized as SUT call"
    );
}

#[test]
fn test_transitive_sut_via_helper() {
    let declared = vec![make_declared("prod_fn", false)];
    let warnings = detect_in(
        r#"
        #[test]
        fn test_via_helper() {
            my_helper();
        }
    "#,
        &declared,
        "fn prod_fn() {}",
        &["my_helper"],
    );
    assert!(
        warnings.is_empty(),
        "my_helper transitively calls prod code"
    );
}

#[test]
fn calls_prod_fn_known_only_via_declared_set() {
    // `prod_only` is a production fn (declared, not a test) that is absent from
    // the parsed scope, so recognition must come from the declared-name set
    // alone — guarding the `!f.is_test` filter and the first `||` branch.
    let declared = vec![make_declared("prod_only", false)];
    let warnings = detect_in(
        r#"
        #[test]
        fn test_it() {
            prod_only();
        }
        "#,
        &declared,
        "fn unrelated() {}",
        &[],
    );
    assert!(
        warnings.is_empty(),
        "a call to a declared production fn is a SUT call"
    );
}

#[test]
fn calls_fn_known_only_via_scope_functions() {
    // `scope_fn` is in the analysed scope but NOT in the declared set; the
    // scope-functions branch alone must recognise it as exercising the SUT.
    let warnings = detect_in(
        r#"
        #[test]
        fn test_it() {
            scope_fn();
        }
        "#,
        &[],
        "fn scope_fn() {}",
        &[],
    );
    assert!(
        warnings.is_empty(),
        "a call to an in-scope function is a SUT call"
    );
}
