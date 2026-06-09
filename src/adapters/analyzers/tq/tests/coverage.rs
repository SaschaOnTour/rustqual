use crate::adapters::analyzers::iosp::{
    compute_severity, Classification, ComplexityMetrics, FunctionAnalysis, LogicOccurrence,
};
use crate::adapters::analyzers::tq::coverage::*;
use crate::adapters::analyzers::tq::lcov::LcovFileData;
use crate::adapters::analyzers::tq::{TqWarning, TqWarningKind};
use std::collections::HashMap;

fn make_func(name: &str, file: &str, line: usize) -> FunctionAnalysis {
    let severity = compute_severity(&Classification::Operation);
    FunctionAnalysis {
        name: name.to_string(),
        file: file.to_string(),
        line,
        classification: Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: None,
        qualified_name: name.to_string(),
        severity,
        cognitive_warning: false,
        cyclomatic_warning: false,
        nesting_depth_warning: false,
        function_length_warning: false,
        unsafe_warning: false,
        error_handling_warning: false,
        complexity_suppressed: false,
        own_calls: vec![],
        parameter_count: 0,
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
    }
}

fn make_lcov_data(fn_hits: &[(&str, u64)], line_hits: &[(usize, u64)]) -> LcovFileData {
    LcovFileData {
        function_hits: fn_hits.iter().map(|(n, c)| (n.to_string(), *c)).collect(),
        line_hits: line_hits.iter().copied().collect(),
    }
}

// ── TQ-004 tests ────────────────────────────────────────

#[test]
fn uncovered_function_is_flagged_regardless_of_name() {
    // A function with zero hits is flagged Uncovered. The second case guards a
    // real regression: a production function merely *named* `test_*` (no test
    // attribute, is_test = false) is real production code and must still be
    // coverage-checked — the old name-prefix heuristic wrongly skipped it.
    for (label, fn_name) in [
        ("ordinary production fn", "process"),
        ("production fn named like a test", "test_connection"),
    ] {
        let results = vec![make_func(fn_name, "src/lib.rs", 10)];
        let mut lcov = HashMap::new();
        lcov.insert(
            "src/lib.rs".to_string(),
            make_lcov_data(&[(fn_name, 0)], &[]),
        );
        let warnings = detect_uncovered_functions(&results, &lcov);
        assert_eq!(warnings.len(), 1, "case {label}");
        assert_eq!(warnings[0].kind, TqWarningKind::Uncovered, "case {label}");
    }
}

#[test]
fn test_covered_function_no_warning() {
    let results = vec![make_func("process", "src/lib.rs", 10)];
    let mut lcov = HashMap::new();
    lcov.insert(
        "src/lib.rs".to_string(),
        make_lcov_data(&[("process", 5)], &[]),
    );
    let warnings = detect_uncovered_functions(&results, &lcov);
    assert!(warnings.is_empty());
}

#[test]
fn test_function_not_in_lcov_no_warning() {
    let results = vec![make_func("process", "src/lib.rs", 10)];
    let lcov = HashMap::new();
    let warnings = detect_uncovered_functions(&results, &lcov);
    assert!(warnings.is_empty());
}

#[test]
fn cfg_excluded_fn_present_in_results_but_absent_from_lcov_not_flagged() {
    // Repro for the wasm-only / cfg-gated coverage discrepancy: rustqual parses
    // source cfg-agnostically (so `excluded_fn` IS in `all_results`), but the
    // coverage run built a target where it was cfg'd out — so the file IS in the
    // LCOV (its sibling `native_fn` was covered) while `excluded_fn` appears in
    // NEITHER function_hits NOR line_hits. It must NOT be reported as Uncovered
    // (TQ-004) or UntestedLogic (TQ-005): "not built in this target" ≠ "0 hits".
    let native = make_func("native_fn", "src/lib.rs", 5);
    let mut excluded = make_func("excluded_fn", "src/lib.rs", 12);
    excluded.complexity = Some(ComplexityMetrics {
        logic_occurrences: vec![LogicOccurrence {
            kind: "if".to_string(),
            line: 13,
        }],
        ..Default::default()
    });
    let results = vec![native, excluded];
    let mut lcov = HashMap::new();
    // Only native_fn is in the coverage artifact (hit + its line); excluded_fn
    // and its lines are entirely absent, exactly as llvm-cov emits when the
    // function is cfg'd out of the built target.
    lcov.insert(
        "src/lib.rs".to_string(),
        make_lcov_data(&[("native_fn", 3)], &[(5, 3)]),
    );
    let uncovered = detect_uncovered_functions(&results, &lcov);
    assert!(
        !uncovered.iter().any(|w| w.function_name == "excluded_fn"),
        "cfg-excluded fn absent from LCOV must not be Uncovered, got {uncovered:?}"
    );
    let untested_logic = detect_untested_logic(&results, &lcov);
    assert!(
        !untested_logic
            .iter()
            .any(|w| w.function_name == "excluded_fn"),
        "cfg-excluded fn's logic lines absent from LCOV must not be UntestedLogic, got {untested_logic:?}"
    );
}

#[test]
fn test_function_excluded_by_is_test_flag() {
    // Test entry points are excluded via the analyzer-computed `is_test`
    // flag (attribute/cfg/path aware), not by a `test_` name prefix —
    // so a `#[tokio::test]`-style fn with any name is excluded.
    let mut func = make_func("five_oh_two_twice", "src/lib.rs", 10);
    func.is_test = true;
    let results = vec![func];
    let mut lcov = HashMap::new();
    lcov.insert(
        "src/lib.rs".to_string(),
        make_lcov_data(&[("five_oh_two_twice", 0)], &[]),
    );
    let warnings = detect_uncovered_functions(&results, &lcov);
    assert!(warnings.is_empty());
}

#[test]
fn test_suppressed_function_excluded() {
    let mut func = make_func("process", "src/lib.rs", 10);
    func.suppressed = true;
    let results = vec![func];
    let mut lcov = HashMap::new();
    lcov.insert(
        "src/lib.rs".to_string(),
        make_lcov_data(&[("process", 0)], &[]),
    );
    let warnings = detect_uncovered_functions(&results, &lcov);
    assert!(warnings.is_empty());
}

// ── TQ-005 tests ────────────────────────────────────────

#[test]
fn untested_logic_in_test_function_excluded() {
    // TQ-005 must also key off the `is_test` flag: uncovered logic in a
    // test function is not a coverage gap to report.
    let mut func = make_func("verifies_retry", "src/lib.rs", 10);
    func.is_test = true;
    func.complexity = Some(ComplexityMetrics {
        logic_occurrences: vec![LogicOccurrence {
            kind: "if".to_string(),
            line: 15,
        }],
        ..Default::default()
    });
    let results = vec![func];
    let mut lcov = HashMap::new();
    lcov.insert("src/lib.rs".to_string(), make_lcov_data(&[], &[(15, 0)]));
    let warnings = detect_untested_logic(&results, &lcov);
    assert!(
        warnings.is_empty(),
        "uncovered logic in a test function must not be flagged: {warnings:?}"
    );
}

#[test]
fn test_untested_logic_detected() {
    let mut func = make_func("process", "src/lib.rs", 10);
    func.complexity = Some(ComplexityMetrics {
        logic_occurrences: vec![LogicOccurrence {
            kind: "if".to_string(),
            line: 15,
        }],
        ..Default::default()
    });
    let results = vec![func];
    let mut lcov = HashMap::new();
    lcov.insert("src/lib.rs".to_string(), make_lcov_data(&[], &[(15, 0)]));
    let warnings = detect_untested_logic(&results, &lcov);
    assert_eq!(warnings.len(), 1);
    match &warnings[0].kind {
        TqWarningKind::UntestedLogic { uncovered_lines } => {
            assert_eq!(uncovered_lines.len(), 1);
            assert_eq!(uncovered_lines[0], ("if".to_string(), 15));
        }
        _ => panic!("expected UntestedLogic"),
    }
}

#[test]
fn test_covered_logic_no_warning() {
    let mut func = make_func("process", "src/lib.rs", 10);
    func.complexity = Some(ComplexityMetrics {
        logic_occurrences: vec![LogicOccurrence {
            kind: "if".to_string(),
            line: 15,
        }],
        ..Default::default()
    });
    let results = vec![func];
    let mut lcov = HashMap::new();
    lcov.insert("src/lib.rs".to_string(), make_lcov_data(&[], &[(15, 3)]));
    let warnings = detect_untested_logic(&results, &lcov);
    assert!(warnings.is_empty());
}

#[test]
fn test_logic_line_not_in_lcov_no_warning() {
    let mut func = make_func("process", "src/lib.rs", 10);
    func.complexity = Some(ComplexityMetrics {
        logic_occurrences: vec![LogicOccurrence {
            kind: "if".to_string(),
            line: 15,
        }],
        ..Default::default()
    });
    let results = vec![func];
    let lcov = HashMap::new(); // no LCOV data at all
    let warnings = detect_untested_logic(&results, &lcov);
    assert!(warnings.is_empty());
}

#[test]
fn test_no_logic_no_warning() {
    let func = make_func("process", "src/lib.rs", 10);
    let results = vec![func];
    let mut lcov = HashMap::new();
    lcov.insert("src/lib.rs".to_string(), make_lcov_data(&[], &[(15, 0)]));
    let warnings = detect_untested_logic(&results, &lcov);
    assert!(warnings.is_empty());
}

#[test]
fn test_multiple_uncovered_logic_lines_one_warning() {
    let mut func = make_func("process", "src/lib.rs", 10);
    func.complexity = Some(ComplexityMetrics {
        logic_occurrences: vec![
            LogicOccurrence {
                kind: "if".to_string(),
                line: 15,
            },
            LogicOccurrence {
                kind: "match".to_string(),
                line: 20,
            },
        ],
        ..Default::default()
    });
    let results = vec![func];
    let mut lcov = HashMap::new();
    lcov.insert(
        "src/lib.rs".to_string(),
        make_lcov_data(&[], &[(15, 0), (20, 0)]),
    );
    let warnings = detect_untested_logic(&results, &lcov);
    assert_eq!(
        warnings.len(),
        1,
        "one warning per function, not per logic line"
    );
    match &warnings[0].kind {
        TqWarningKind::UntestedLogic { uncovered_lines } => {
            assert_eq!(uncovered_lines.len(), 2);
        }
        _ => panic!("expected UntestedLogic"),
    }
}
