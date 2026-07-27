//! TQ-003: production functions no test reaches.
//!
//! Shared fixtures and the per-declaration exclusions live here; the two
//! halves with their own shape sit next to them — `call_graph` (what
//! reachability through calls and macros finds) and `coverage` (what a
//! report adds, and which findings it actually answered).

mod call_graph;
mod coverage;

use crate::adapters::analyzers::dry::dead_code::DeadCodeWarning;
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
        dead_code_exempt: false,
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
    detect_untested_functions(&declared, &prod_calls, &tested, &[], &HashSet::new())
}

#[test]
fn test_untested_prod_fn_emits_warning() {
    let declared = vec![make_declared("process", false)];
    let prod_calls: HashSet<String> = ["process".to_string()].into();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[], &HashSet::new());
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].kind,
        TqWarningKind::Untested { measured: false }
    );
    assert_eq!(warnings[0].function_name, "process");
}

#[test]
fn test_tested_fn_no_warning() {
    let declared = vec![make_declared("process", false)];
    let prod_calls: HashSet<String> = ["process".to_string()].into();
    let tested: HashSet<String> = ["process".to_string()].into();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[], &HashSet::new());
    assert!(warnings.is_empty());
}

#[test]
fn test_uncalled_fn_no_warning() {
    let declared = vec![make_declared("unused", false)];
    let prod_calls: HashSet<String> = HashSet::new();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[], &HashSet::new());
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

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[], &HashSet::new());
    assert!(warnings.is_empty());
}

#[test]
fn test_main_fn_excluded() {
    let mut declared = vec![make_declared("main", false)];
    declared[0].is_main = true;
    let prod_calls: HashSet<String> = ["main".to_string()].into();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[], &HashSet::new());
    assert!(warnings.is_empty());
}

#[test]
fn test_api_fn_excluded() {
    let mut declared = vec![make_declared("handle_overview", false)];
    declared[0].is_api = true;
    let prod_calls: HashSet<String> = ["handle_overview".to_string()].into();
    let tested = HashSet::new();

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[], &HashSet::new());
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

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[], &HashSet::new());
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

    let warnings = detect_untested_functions(&declared, &prod_calls, &tested, &[], &HashSet::new());
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

    let warnings =
        detect_untested_functions(&declared, &prod_calls, &tested, &dead, &HashSet::new());
    assert!(warnings.is_empty());
}

// ── Transitive closure tests ─────────────────────────────────────────

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
        &HashSet::new(),
    );
    assert!(out.is_empty(), "no production call, no TQ-003: {out:?}");
}
