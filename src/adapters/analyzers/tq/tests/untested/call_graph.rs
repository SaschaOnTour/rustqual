//! Reachability: transitive calls, cycles, and edges recovered from macros.

use std::collections::{HashMap, HashSet};

use super::{make_declared, untested_via_graph};
use crate::adapters::analyzers::tq::build_reaches_prod_set;
use crate::adapters::analyzers::tq::untested::*;

#[test]
fn test_transitive_tested_not_flagged() {
    // Test calls A, A calls B → B should not be flagged
    let warnings = untested_via_graph(&["a", "b"], &["a", "b"], &["a"], &[("a", "b")]);
    assert!(warnings.is_empty(), "b is transitively tested via a");
}

#[test]
fn test_deep_transitive_not_flagged() {
    // Test calls A, A→B→C → C should not be flagged
    let warnings = untested_via_graph(
        &["a", "b", "c"],
        &["a", "b", "c"],
        &["a"],
        &[("a", "b"), ("b", "c")],
    );
    assert!(warnings.is_empty(), "c is transitively tested via a→b→c");
}

#[test]
fn test_circular_calls_no_infinite_loop() {
    // A→B→A (cycle), test calls A → terminates without infinite loop
    let warnings = untested_via_graph(&["a", "b"], &["a", "b"], &["a"], &[("a", "b"), ("b", "a")]);
    assert!(
        warnings.is_empty(),
        "cycle terminates; both a and b are tested"
    );
}

#[test]
fn test_untested_leaf_still_flagged() {
    // Test calls A, A calls B, but D is never called transitively → D flagged
    let warnings = untested_via_graph(&["a", "b", "d"], &["a", "b", "d"], &["a"], &[("a", "b")]);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].function_name, "d");
}

#[test]
fn test_empty_call_graph_falls_back_to_direct() {
    // Empty call graph → only directly tested functions are cleared
    let declared = vec![make_declared("a", false), make_declared("b", false)];
    let prod_calls: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
    let tested: HashSet<String> = ["a".to_string()].into();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[], &HashSet::new());
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].function_name, "b");
}

#[test]
fn build_reaches_prod_set_seeds_prod_and_walks_callers_backward() {
    // call graph: test_a → helper → prod_fn. Backward BFS from the production
    // seed must reach helper (calls prod_fn) and test_a (calls helper).
    let mut call_graph: HashMap<String, Vec<String>> = HashMap::new();
    call_graph.insert("helper".to_string(), vec!["prod_fn".to_string()]);
    call_graph.insert("test_a".to_string(), vec!["helper".to_string()]);
    // `lonely_test` reaches no production code; the `!is_test` seed filter must
    // keep it out of the set.
    let declared = vec![
        make_declared("prod_fn", false),
        make_declared("lonely_test", true),
    ];
    let reaches = build_reaches_prod_set(&call_graph, &declared);
    assert!(reaches.contains("prod_fn"), "production fn seeds the set");
    assert!(reaches.contains("helper"), "helper calls prod_fn");
    assert!(
        reaches.contains("test_a"),
        "test_a transitively reaches prod via helper"
    );
    assert!(
        !reaches.contains("lonely_test"),
        "a test fn reaching no prod code is excluded from the seed"
    );
}

#[test]
fn full_call_graph_recovers_repeat_form_macro_call_edge() {
    // `caller`'s only call to `helper` is inside a `vec![helper(); 3]` repeat
    // macro. The comma-expr parse fails on the `;`; the shared recovery's block
    // fallback must still produce a `caller → helper` edge so TQ-003
    // reachability flows through macro-wrapped calls.
    let src = "fn helper() {} fn caller() { let _ = vec![helper(); 3]; }";
    let ast = syn::parse_file(src).expect("parse");
    let parsed = vec![("lib.rs".to_string(), src.to_string(), ast)];
    let graph = crate::adapters::analyzers::tq::build_full_call_graph(&parsed);
    assert!(
        graph
            .get("caller")
            .is_some_and(|callees| callees.iter().any(|c| c == "helper")),
        "caller must have a macro-embedded edge to helper, got: {:?}",
        graph.get("caller")
    );
}
