use super::*;

#[test]
fn test_pure_integration_unchanged() {
    // Integration (only calls, no logic) stays Integration — unaffected by leaf detection.
    let code = r#"
        fn step_a() {}
        fn step_b() {}
        fn pipeline() {
            step_a();
            step_b();
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "pipeline").unwrap();
    assert!(
        matches!(f.classification, Classification::Integration),
        "Pure Integration should stay Integration, got {:?}",
        f.classification
    );
}

#[test]
fn test_cascading_leaf_detection() {
    // step_a and step_b are leaves (C=0).
    // middle calls only leaves → after leaf detection, middle is Operation → also a leaf.
    // top calls middle + has logic → should be Operation (middle is transitively a leaf).
    let code = r#"
        fn step_a() -> i32 { if true { 1 } else { 0 } }
        fn step_b() -> i32 { 42 }
        fn middle() -> i32 {
            if step_a() > 0 { step_b() } else { 0 }
        }
        fn top(x: bool) -> i32 {
            if x { middle() } else { -1 }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "top").unwrap();
    assert!(
        matches!(f.classification, Classification::Operation),
        "Cascading leaf: top calls middle (which calls only leaves) should be Operation, got {:?}",
        f.classification
    );
}

// ---------------------------------------------------------------
// qual:recursive annotation tests
// ---------------------------------------------------------------

#[test]
fn test_recursive_annotation_makes_self_call_safe() {
    let code = r#"
        // qual:recursive
        fn traverse(node: &str) -> i32 {
            if node.is_empty() { return 0; }
            traverse(node)
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "traverse").unwrap();
    assert!(
        matches!(f.classification, Classification::Operation),
        "qual:recursive should make self-call safe → Operation, got {:?}",
        f.classification
    );
}

#[test]
fn test_recursive_without_annotation_is_violation() {
    let code = r#"
        fn inner() {}
        fn traverse(node: &str) -> i32 {
            if node.is_empty() { return 0; }
            inner();
            traverse(node)
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "traverse").unwrap();
    assert!(
        matches!(f.classification, Classification::Violation { .. }),
        "Without annotation, recursive + non-leaf call + logic = Violation, got {:?}",
        f.classification
    );
}

// ---------------------------------------------------------------
// Integration-as-safe-target tests
// ---------------------------------------------------------------

#[test]
fn test_call_to_integration_is_safe() {
    let code = r#"
        fn log_action() {}
        fn db_save() { log_action(); }
        fn handler(x: bool) -> i32 {
            if x { db_save(); 1 } else { 0 }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "handler").unwrap();
    assert!(
        matches!(f.classification, Classification::Operation),
        "Call to Integration (L=0, C>0) + logic should be Operation, got {:?}",
        f.classification
    );
}

#[test]
fn test_call_to_violation_stays_violation() {
    let code = r#"
        fn bad_a(x: bool) -> i32 {
            if x { bad_b(false) } else { 0 }
        }
        fn bad_b(x: bool) -> i32 {
            if x { bad_a(true) } else { 1 }
        }
        fn caller(y: bool) -> i32 {
            if y { bad_a(true) } else { -1 }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "caller").unwrap();
    assert!(
        matches!(f.classification, Classification::Violation { .. }),
        "Call to mutually-recursive Violation + logic should stay Violation, got {:?}",
        f.classification
    );
}

#[test]
fn test_mixed_leaf_and_integration_calls_safe() {
    let code = r#"
        fn log_it() {}
        fn get_config() -> i32 { if true { 1 } else { 2 } }
        fn db_fetch() -> i32 { log_it(); 42 }
        fn process(x: bool) -> i32 {
            let cfg = get_config();
            if x { db_fetch() + cfg } else { cfg }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "process").unwrap();
    assert!(
        matches!(f.classification, Classification::Operation),
        "Calls to leaf + integration + logic should be Operation, got {:?}",
        f.classification
    );
}

#[test]
fn own_call_only_inside_macro_is_not_counted_deliberate_safe_direction() {
    // DELIBERATE non-fix (macro-token-blindness sweep): IOSP own-call counting
    // does NOT descend into macro bodies — `iosp::visitor::walk_macro` only
    // counts panic!/todo! for error-handling, never own-calls. So `helper()`,
    // reachable only inside `vec![helper(); 1]`, is absent from `own_calls`.
    //
    // This is the safe direction (under-reports, never false-positive) and is
    // kept on purpose: descending would newly flag real macro-driven render
    // code — e.g. a dioxus component that renders its own components inside
    // `rsx!` while holding a conditional — as an IOSP Violation. That is a
    // breaking behaviour change, out of scope for fixing the macro *call-graph*
    // blindness (dead-code / TQ / SRP / architecture), which only ever removes
    // false positives. Flipping this must be a separately-scoped decision; this
    // test guards the boundary.
    let code = r#"
        fn helper() -> i32 { 42 }
        fn renders(flag: bool) -> Vec<i32> {
            if flag {
                vec![helper(); 1]
            } else {
                vec![]
            }
        }
    "#;
    let results = parse_and_analyze(code);
    let f = results.iter().find(|f| f.name == "renders").unwrap();
    assert!(
        !f.own_calls.iter().any(|c| c == "helper"),
        "IOSP deliberately does not count an own-call hidden in a macro body; \
         got own_calls = {:?}",
        f.own_calls
    );
}
