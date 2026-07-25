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
fn tq_no_sut_abc_triangulation() {
    // A/B/C triangulation isolating the SUT mapper from macro blindness.
    //   A — plain call: has a SUT → must NOT fire TQ_NO_SUT (rule baseline).
    //   B — call only inside a comma-expr macro (`vec![target_b()]`): the SUT
    //       collector re-parses macro tokens as an expr list, so this IS seen →
    //       must NOT fire. (If it fired, TQ_NO_SUT would be a pure macro cascade.)
    //   C — genuinely no SUT: TQ_NO_SUT MUST fire (rule works).
    let declared = vec![
        make_declared("target_a", false),
        make_declared("target_b", false),
    ];
    let warnings = detect_in(
        r#"
        #[test]
        fn test_a() { assert_eq!(target_a(), 7); }
        #[test]
        fn test_b() { assert_eq!(vec![target_b()][0], 7); }
        #[test]
        fn test_c() { assert_eq!(1 + 1, 2); }
        "#,
        &declared,
        "fn target_a() -> i32 { 7 } fn target_b() -> i32 { 7 }",
        &[],
    );
    let fired: Vec<&str> = warnings.iter().map(|w| w.function_name.as_str()).collect();
    assert_eq!(
        fired,
        vec!["test_c"],
        "only test_c (no SUT) must fire; A=plain B=comma-macro are real SUT calls, got {fired:?}"
    );
}

#[test]
fn tq_no_sut_satisfied_by_component_render_in_dsl_macro() {
    // A UI test whose SUT call is rendering a component inside an `rsx!` DSL
    // body (which parses as no structured exprs). The raw ident fallback must
    // let the component count as the SUT, so NO_SUT does not fire — a real-world
    // rsx! component-render symptom.
    let declared = vec![make_declared("LiveLogViewer", false)];
    let warnings = detect_in(
        r#"
        #[test]
        fn live_log_renders() {
            let _ = rsx! { div { class: "log", LiveLogViewer {} } };
        }
        "#,
        &declared,
        "fn LiveLogViewer() {}",
        &[],
    );
    assert!(
        warnings.is_empty(),
        "rendering a component inside an rsx! DSL must satisfy SUT, got {warnings:?}"
    );
}

#[test]
fn tq_no_sut_satisfied_by_repeat_macro_call() {
    // Regression guard for the repeat-macro SUT blindness fixed in the
    // macro-token sweep. The SUT mapper routes macro bodies through
    // `recover_exprs` (in `test_references.rs`), whose block fallback handles the
    // `;`-repeat form a comma-only parse missed. So `target()` inside
    // `vec![target(); 1]` IS a real SUT call and TQ_NO_SUT must stay silent.
    let declared = vec![make_declared("target", false)];
    let warnings = detect_in(
        r#"
        #[test]
        fn test_repeat() { assert_eq!(vec![target(); 1][0], 7); }
        "#,
        &declared,
        "fn target() -> i32 { 7 }",
        &[],
    );
    assert!(
        warnings.is_empty(),
        "target() called inside a vec![_; n] repeat macro is a real SUT call, got {warnings:?}"
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
