use crate::adapters::analyzers::iosp::{CallOccurrence, LogicOccurrence};
use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};
use crate::adapters::report::test_support::make_result;
use crate::report::suggestions::*;

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
