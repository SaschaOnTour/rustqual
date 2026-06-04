use super::*;

/// A pure match-dispatch fn (every arm is a single delegation). Shared by the
/// classification test and the "complexity is still tracked" test — two
/// distinct contracts on the same canonical shape.
const MATCH_DISPATCH: &str = r#"
    fn call_a() {}
    fn call_b() {}
    fn dispatch(x: i32) {
        match x {
            0 => call_a(),
            _ => call_b(),
        }
    }
"#;

#[test]
fn test_trivial_self_getter_not_violation() {
    let code = r#"
        struct Counter { count: usize }
        impl Counter {
            fn symbol_count(&self) -> usize { self.count }
            fn next_symbol(&self) -> usize {
                if self.symbol_count() > 0 {
                    self.symbol_count() + 1
                } else {
                    0
                }
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let next = results.iter().find(|r| r.name == "next_symbol").unwrap();
    assert_eq!(
        next.classification,
        Classification::Operation,
        "Trivial getter should not make next_symbol a Violation, got {:?}",
        next.classification
    );
}

// ---------------------------------------------------------------
// Bug Fix: Type::new() Not Own Call
// ---------------------------------------------------------------

#[test]
fn test_leaf_constructor_call_is_operation() {
    // Adx::new() is Trivial (leaf). Calling a leaf + logic = Operation.
    let code = r#"
        struct Adx { period: usize }
        impl Adx {
            fn new(period: usize) -> Self { Adx { period } }
        }
        fn compute(data: &[f64]) -> f64 {
            let indicator = Adx::new(14);
            if data.is_empty() { 0.0 } else { data[0] }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "compute").unwrap();
    assert!(
        matches!(f.classification, Classification::Operation),
        "Adx::new() is leaf → calling it + logic = Operation, got {:?}",
        f.classification
    );
}

// ---------------------------------------------------------------
// Bug Fix: Trivial .get() Getter Not Violation
// ---------------------------------------------------------------

#[test]
fn test_trivial_getter_get_not_violation() {
    let code = r#"
        struct Browser { results: Vec<String>, selected: usize }
        impl Browser {
            fn current(&self) -> Option<&String> { self.results.get(self.selected) }
            fn process(&self) -> String {
                if let Some(item) = self.current() {
                    item.clone()
                } else {
                    String::new()
                }
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "process").unwrap();
    assert_eq!(
        f.classification,
        Classification::Operation,
        "Trivial .get() getter should not make process a Violation, got {:?}",
        f.classification
    );
}

// ---------------------------------------------------------------
// Bug Fix: For-Loop Delegation Not Violation
// ---------------------------------------------------------------

#[test]
fn test_for_loop_delegation_not_violation() {
    let code = r#"
        fn process(_x: i32) {}
        fn f(items: Vec<i32>) {
            for x in items {
                process(x);
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "f").unwrap();
    assert_eq!(
        f.classification,
        Classification::Integration,
        "For-loop delegation should be Integration, got {:?}",
        f.classification
    );
}

// ---------------------------------------------------------------
// Bug Fix: Match-Dispatch Delegation Not Violation
// ---------------------------------------------------------------

#[test]
fn test_match_dispatch_is_integration() {
    let results = parse_and_analyze(MATCH_DISPATCH);
    let f = results.iter().find(|r| r.name == "dispatch").unwrap();
    assert_eq!(
        f.classification,
        Classification::Integration,
        "Match dispatch should be Integration, got {:?}",
        f.classification
    );
}

#[test]
fn test_match_dispatch_method_is_integration() {
    let code = r#"
        struct S;
        impl S {
            fn run_a(&self) {}
            fn run_b(&self) {}
            fn dispatch(&self, x: i32) {
                match x {
                    0 => self.run_a(),
                    _ => self.run_b(),
                }
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "dispatch").unwrap();
    assert_eq!(
        f.classification,
        Classification::Integration,
        "Match method dispatch should be Integration, got {:?}",
        f.classification
    );
}

#[test]
fn test_match_with_logic_in_arm_is_violation() {
    let code = r#"
        fn call_a(_x: i32) { if _x > 0 { dispatch(_x - 1); } }
        fn call_b() { dispatch(0); }
        fn dispatch(x: i32) {
            match x {
                0 => call_a(x + 1),
                _ => call_b(),
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "dispatch").unwrap();
    assert!(
        matches!(f.classification, Classification::Violation { .. }),
        "Match with logic in arm should be Violation, got {:?}",
        f.classification
    );
}

#[test]
fn test_match_with_guard_is_violation() {
    let code = r#"
        fn call_a() { if true { dispatch(0); } }
        fn call_b() { dispatch(1); }
        fn dispatch(x: i32) {
            match x {
                n if n > 0 => call_a(),
                _ => call_b(),
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|r| r.name == "dispatch").unwrap();
    assert!(
        matches!(f.classification, Classification::Violation { .. }),
        "Match with guard should be Violation, got {:?}",
        f.classification
    );
}

#[test]
fn test_match_dispatch_complexity_still_tracked() {
    // Leniency reclassifies the match as orchestration but must NOT zero out
    // the cognitive complexity — the match arms are still decision points.
    let results = parse_and_analyze(MATCH_DISPATCH);
    let f = results.iter().find(|r| r.name == "dispatch").unwrap();
    assert!(
        f.complexity.as_ref().unwrap().cognitive_complexity >= 1,
        "Complexity should still be tracked for dispatch match"
    );
}

// ---------------------------------------------------------------
// is_test detection tests
// ---------------------------------------------------------------
