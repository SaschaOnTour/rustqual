use crate::adapters::analyzers::dry::dead_code::DeadCodeWarning;
use crate::adapters::analyzers::tq::build_reaches_prod_set;
use crate::adapters::analyzers::tq::untested::*;
use crate::adapters::analyzers::tq::{TqWarning, TqWarningKind};
use crate::adapters::shared::declared_function::DeclaredFunction;
use std::collections::{HashMap, HashSet};

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

/// Build the declared / prod-call / test-call / call-graph inputs, derive the
/// transitive tested set, and run TQ-untested detection. Collapses the shared
/// arrange across the transitive-coverage tests (each `edges` entry is one
/// `from → to` call; each `from` is unique in these fixtures).
fn untested_via_graph(
    declared: &[&str],
    prod: &[&str],
    test: &[&str],
    edges: &[(&str, &str)],
) -> Vec<TqWarning> {
    let declared: Vec<DeclaredFunction> =
        declared.iter().map(|n| make_declared(n, false)).collect();
    let prod_calls: HashSet<String> = prod.iter().map(|s| s.to_string()).collect();
    let test_calls: HashSet<String> = test.iter().map(|s| s.to_string()).collect();
    let call_graph: HashMap<String, Vec<String>> = edges
        .iter()
        .map(|(f, t)| (f.to_string(), vec![t.to_string()]))
        .collect();
    let tested = build_transitive_tested_set(&test_calls, &call_graph);
    detect_untested_functions(&declared, &prod_calls, &tested, &[])
}

#[test]
fn test_untested_prod_fn_emits_warning() {
    let declared = vec![make_declared("process", false)];
    let prod_calls: HashSet<String> = ["process".to_string()].into();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[]);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind, TqWarningKind::Untested);
    assert_eq!(warnings[0].function_name, "process");
}

#[test]
fn test_tested_fn_no_warning() {
    let declared = vec![make_declared("process", false)];
    let prod_calls: HashSet<String> = ["process".to_string()].into();
    let tested: HashSet<String> = ["process".to_string()].into();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[]);
    assert!(warnings.is_empty());
}

#[test]
fn test_uncalled_fn_no_warning() {
    let declared = vec![make_declared("unused", false)];
    let prod_calls: HashSet<String> = HashSet::new();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[]);
    assert!(
        warnings.is_empty(),
        "functions not called from prod are not TQ-003"
    );
}

#[test]
fn test_test_fn_excluded() {
    let declared = vec![make_declared("test_helper", true)];
    let prod_calls: HashSet<String> = ["test_helper".to_string()].into();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[]);
    assert!(warnings.is_empty());
}

#[test]
fn test_main_fn_excluded() {
    let mut declared = vec![make_declared("main", false)];
    declared[0].is_main = true;
    let prod_calls: HashSet<String> = ["main".to_string()].into();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[]);
    assert!(warnings.is_empty());
}

#[test]
fn test_api_fn_excluded() {
    let mut declared = vec![make_declared("handle_overview", false)];
    declared[0].is_api = true;
    let prod_calls: HashSet<String> = ["handle_overview".to_string()].into();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[]);
    assert!(
        warnings.is_empty(),
        "qual:api functions should be excluded from TQ-003"
    );
}

#[test]
fn test_test_helper_fn_excluded() {
    let mut declared = vec![make_declared("shared_asserter", false)];
    declared[0].is_test_helper = true;
    let prod_calls: HashSet<String> = ["shared_asserter".to_string()].into();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[]);
    assert!(
        warnings.is_empty(),
        "qual:test_helper functions should be excluded from TQ-003"
    );
}

#[test]
fn test_trait_impl_excluded() {
    let mut declared = vec![make_declared("fmt", false)];
    declared[0].is_trait_impl = true;
    let prod_calls: HashSet<String> = ["fmt".to_string()].into();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[]);
    assert!(warnings.is_empty());
}

#[test]
fn test_dead_code_excluded() {
    let declared = vec![make_declared("dead_fn", false)];
    let prod_calls: HashSet<String> = ["dead_fn".to_string()].into();
    let tested = HashSet::new();
    let dead = vec![
        crate::adapters::analyzers::dry::dead_code::DeadCodeWarning {
            function_name: "dead_fn".to_string(),
            file: "lib.rs".to_string(),
            line: 1,
            kind: crate::adapters::analyzers::dry::dead_code::DeadCodeKind::Uncalled,
            qualified_name: "dead_fn".to_string(),
            suggestion: String::new(),
        },
    ];

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &dead);
    assert!(warnings.is_empty());
}

// ── Transitive closure tests ─────────────────────────────────────────

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

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[]);
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

#[test]
fn a_reexport_alone_is_not_a_production_call() {
    // `pub use suites::test_thing;` records `test_thing` as production usage so
    // DRY-002 does not call it dead. TQ-003 asks a different question — does
    // production *call* it — and a re-export is not a call. Treating it as one
    // makes every re-exported entry point a candidate, and if the real caller is
    // a macro the tool cannot expand, the candidate becomes a false "untested".
    let declared = [make_declared("test_thing", false)];
    let out = detect_untested_functions(
        &declared,
        &HashSet::new(),
        &HashSet::new(),
        &[] as &[DeadCodeWarning],
    );
    assert!(out.is_empty(), "no production call, no TQ-003: {out:?}");
}

#[test]
fn coverage_seeds_the_tested_set() {
    // `FNDA:1,name` says a test ran the function. That is measurement, not
    // inference — it settles the cases the call graph cannot follow (a macro it
    // does not expand, a trait object, generated code) without any heuristic.
    let hits: HashMap<String, u64> = [("ran".to_string(), 3u64), ("never".to_string(), 0u64)]
        .into_iter()
        .collect();
    let data = crate::adapters::analyzers::tq::lcov::LcovFileData {
        function_hits: hits,
        line_hits: HashMap::new(),
    };
    let files: HashMap<String, _> = [("src/lib.rs".to_string(), data)].into_iter().collect();
    let seeded = crate::adapters::analyzers::tq::executed_under_test(Some(&files));
    assert!(seeded.contains(&"ran".to_string()));
    assert!(
        !seeded.contains(&"never".to_string()),
        "a recorded-but-never-executed function is not tested: {seeded:?}"
    );
    assert!(
        crate::adapters::analyzers::tq::executed_under_test(None).is_empty(),
        "no report, no change to the call-graph answer"
    );
}
