use crate::adapters::analyzers::iosp::{compute_severity, CallOccurrence, LogicOccurrence};
use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};
use crate::report::suggestions::*;

fn make_result(name: &str, classification: Classification) -> FunctionAnalysis {
    let severity = compute_severity(&classification);
    FunctionAnalysis {
        name: name.to_string(),
        file: "test.rs".to_string(),
        line: 1,
        classification,
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

#[test]
fn test_print_suggestions_no_violations() {
    let results = vec![
        make_result("a", Classification::Integration),
        make_result("b", Classification::Operation),
    ];
    print_suggestions(&results);
}

#[test]
fn test_print_suggestions_with_if_logic() {
    let results = vec![make_result(
        "if_fn",
        Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![LogicOccurrence {
                kind: "if".into(),
                line: 1,
            }],
            call_locations: vec![CallOccurrence {
                name: "helper".into(),
                line: 2,
            }],
        },
    )];
    print_suggestions(&results);
}

#[test]
fn test_print_suggestions_with_loop_logic() {
    let results = vec![make_result(
        "loop_fn",
        Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![LogicOccurrence {
                kind: "for".into(),
                line: 1,
            }],
            call_locations: vec![CallOccurrence {
                name: "helper".into(),
                line: 2,
            }],
        },
    )];
    print_suggestions(&results);
}

#[test]
fn test_print_suggestions_with_arithmetic_logic() {
    let results = vec![make_result(
        "arith_fn",
        Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![LogicOccurrence {
                kind: "arithmetic".into(),
                line: 1,
            }],
            call_locations: vec![CallOccurrence {
                name: "helper".into(),
                line: 2,
            }],
        },
    )];
    print_suggestions(&results);
}

#[test]
fn test_print_suggestions_suppressed_skipped() {
    let mut func = make_result(
        "suppressed_fn",
        Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![LogicOccurrence {
                kind: "if".into(),
                line: 1,
            }],
            call_locations: vec![CallOccurrence {
                name: "f".into(),
                line: 2,
            }],
        },
    );
    func.suppressed = true;
    let results = vec![func];
    print_suggestions(&results);
}
