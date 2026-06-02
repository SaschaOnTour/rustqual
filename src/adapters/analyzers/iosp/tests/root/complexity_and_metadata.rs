use super::*;

#[test]
fn test_complexity_metrics_present() {
    let code = r#"
        fn f(x: i32) {
            let _y = x;
            if x > 0 {
                if x > 10 {
                    let _ = x + 1;
                }
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "f").unwrap();
    let metrics = f
        .complexity
        .as_ref()
        .expect("Should have complexity metrics");
    assert!(metrics.logic_count > 0, "Should have logic count");
    assert!(metrics.max_nesting > 0, "Should have nesting depth");
}

#[test]
fn test_complexity_nesting_depth() {
    let code = r#"
        fn f(x: i32) {
            let _y = x;
            if x > 0 {
                if x > 10 {
                    while x > 100 {
                        break;
                    }
                }
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "f").unwrap();
    let metrics = f.complexity.as_ref().unwrap();
    assert_eq!(
        metrics.max_nesting, 3,
        "Expected nesting depth 3 (if > if > while)"
    );
}

// ---------------------------------------------------------------
// C2: Severity Tests
// ---------------------------------------------------------------

#[test]
fn test_severity_low() {
    let code = r#"
        fn helper(x: bool) { if x { f(false); } }
        fn f(x: bool) {
            let _y = x;
            if x { helper(true); }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "f").unwrap();
    assert_eq!(f.severity, Some(Severity::Low));
}

#[test]
fn test_severity_none_for_non_violation() {
    let code = r#"
        fn f(x: i32) {
            let _y = x;
            if x > 0 { }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "f").unwrap();
    assert_eq!(f.severity, None);
}

// ---------------------------------------------------------------
// Suppression Tests
// ---------------------------------------------------------------

#[test]
fn test_suppressed_flag_default_false() {
    let code = r#"
        fn f() {}
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "f").unwrap();
    assert!(!f.suppressed);
}

// ---------------------------------------------------------------
// D1/D7: qualified_name + severity fields
// ---------------------------------------------------------------

#[test]
fn test_qualified_name_free_fn() {
    let code = r#"
        fn my_function() {}
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "my_function").unwrap();
    assert_eq!(f.qualified_name, "my_function");
}

#[test]
fn test_qualified_name_impl_method() {
    let code = r#"
        struct Foo;
        impl Foo {
            fn bar(&self) {}
        }
    "#;
    let results = parse_and_analyze(code);
    let bar = results.iter().find(|r| r.name == "bar").unwrap();
    assert_eq!(bar.qualified_name, "Foo::bar");
}

// ---------------------------------------------------------------
// Bug Fix: Trivial Self-Getter Not Violation
// ---------------------------------------------------------------
