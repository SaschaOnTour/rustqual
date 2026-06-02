use super::*;

#[test]
fn test_violation_locations() {
    let code = r#"fn helper(x: i32) { if x > 0 { violator(x); } }
fn violator(x: i32) {
let _y = x;
if _y > 0 {
    helper(_y);
}
}
"#;
    let results = parse_and_analyze(code);
    let v = results.iter().find(|r| r.name == "violator").unwrap();
    if let Classification::Violation {
        logic_locations,
        call_locations,
        ..
    } = &v.classification
    {
        assert!(
            logic_locations
                .iter()
                .any(|l| l.kind == "if" && l.line == 4),
            "Expected 'if' on line 4, got: {:?}",
            logic_locations
        );
        assert!(
            call_locations
                .iter()
                .any(|c| c.name == "helper" && c.line == 5),
            "Expected 'helper' call on line 5, got: {:?}",
            call_locations
        );
    } else {
        panic!("Expected Violation, got {:?}", v.classification);
    }
}

// ---------------------------------------------------------------
// Structure Tests
// ---------------------------------------------------------------

#[test]
fn test_impl_block_parent_type() {
    let code = r#"
        struct Foo;
        impl Foo {
            fn bar(&self) {}
        }
    "#;
    let results = parse_and_analyze(code);
    let bar = results.iter().find(|r| r.name == "bar").unwrap();
    assert_eq!(bar.parent_type, Some("Foo".to_string()));
}

#[test]
fn test_trait_default_impl() {
    let code = r#"
        fn step() {}
        trait MyTrait {
            fn default_method(&self) {
                step();
                step();
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let dm = results.iter().find(|r| r.name == "default_method").unwrap();
    assert_eq!(dm.classification, Classification::Integration);
    assert_eq!(dm.parent_type, Some("MyTrait".to_string()));
}

#[test]
fn test_ignored_function_skipped() {
    let mut config = Config::default();
    config.ignore_functions.push("test_*".to_string());
    let code = r#"
        fn test_something() {
            if true { }
        }
        fn real_function() -> i32 { 42 }
    "#;
    let results = parse_and_analyze_with_config(code, &config);
    assert!(
        results.iter().all(|r| r.name != "test_something"),
        "Ignored function should not appear in results"
    );
    assert!(
        results.iter().any(|r| r.name == "real_function"),
        "Non-ignored function should appear in results"
    );
}

#[test]
fn test_nested_module() {
    let code = r#"
        mod inner {
            fn nested_fn() -> i32 { 42 }
        }
    "#;
    let results = parse_and_analyze(code);
    let nested = results.iter().find(|r| r.name == "nested_fn").unwrap();
    assert_eq!(nested.classification, Classification::Trivial);
}

// ---------------------------------------------------------------
// A2: Recursion Tests
// ---------------------------------------------------------------

#[test]
fn test_recursion_default_is_violation() {
    let code = r#"
        fn fib(n: u32) -> u32 {
            let _x = n;
            if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
        }
    "#;
    let results = parse_and_analyze(code);
    let fib = results.iter().find(|r| r.name == "fib").unwrap();
    assert!(
        matches!(fib.classification, Classification::Violation { .. }),
        "Recursive function should be Violation by default, got {:?}",
        fib.classification
    );
}

#[test]
fn test_recursion_allowed_becomes_operation() {
    let mut config = Config::default();
    config.allow_recursion = true;
    let code = r#"
        fn fib(n: u32) -> u32 {
            let _x = n;
            if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
        }
    "#;
    let results = parse_and_analyze_with_config(code, &config);
    let fib = results.iter().find(|r| r.name == "fib").unwrap();
    assert_eq!(
        fib.classification,
        Classification::Operation,
        "Recursive function with allow_recursion should be Operation, got {:?}",
        fib.classification
    );
}

// ---------------------------------------------------------------
// A3: Error Propagation Tests
// ---------------------------------------------------------------

#[test]
fn test_question_mark_default_not_logic() {
    let code = r#"
        fn f() -> Result<(), String> {
            let _x = 1;
            let _y: Result<i32, String> = Ok(1);
            Ok(())
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "f").unwrap();
    assert!(
        !matches!(f.classification, Classification::Violation { .. }),
        "? operator should not count as logic by default"
    );
}

#[test]
fn test_question_mark_strict_counts_as_logic() {
    let mut config = Config::default();
    config.strict_error_propagation = true;
    let code = r#"
        fn helper() -> Result<i32, String> { Ok(42) }
        fn f() -> Result<(), String> {
            let _x = helper()?;
            let _ = 1;
            Ok(())
        }
    "#;
    let results = parse_and_analyze_with_config(code, &config);
    let f = results.iter().find(|r| r.name == "f").unwrap();
    assert!(
        matches!(f.classification, Classification::Violation { .. }),
        "? operator should count as logic with strict_error_propagation, got {:?}",
        f.classification
    );
}

// ---------------------------------------------------------------
// A4: Async/Await Tests
// ---------------------------------------------------------------

#[test]
fn test_async_block_lenient_ignores_logic() {
    let code = r#"
        fn f() {
            let _ = async { if true { 1 } else { 2 } };
            let _ = 1;
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "f").unwrap();
    assert!(
        !matches!(f.classification, Classification::Violation { .. }),
        "Logic inside async block should be ignored in lenient mode, got {:?}",
        f.classification
    );
}

// ---------------------------------------------------------------
// G1: Complexity Metrics Tests
// ---------------------------------------------------------------
