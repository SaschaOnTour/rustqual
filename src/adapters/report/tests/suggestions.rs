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

fn violation(logic_kind: &str, with_calls: bool) -> FunctionAnalysis {
    make_result(
        "v",
        Classification::Violation {
            has_logic: true,
            has_own_calls: with_calls,
            logic_locations: vec![LogicOccurrence {
                kind: logic_kind.into(),
                line: 1,
            }],
            call_locations: if with_calls {
                vec![CallOccurrence {
                    name: "helper".into(),
                    line: 2,
                }]
            } else {
                vec![]
            },
        },
    )
}

#[test]
fn suggestion_messages_pick_branch_by_logic_kind_and_calls() {
    assert!(
        suggestion_messages(&violation("if", true))[0]
            .0
            .contains("Extract the condition"),
        "conditional + calls → condition suggestion"
    );
    assert!(
        suggestion_messages(&violation("for", true))[0]
            .0
            .contains("iterator chain"),
        "loop + calls → iterator suggestion"
    );
    // Pure logic (no conditional/loop) → the generic extract-logic suggestion,
    // regardless of calls.
    assert!(
        suggestion_messages(&violation("let", false))[0]
            .0
            .contains("Extract the logic"),
        "no conditional/loop → extract-logic suggestion"
    );
    // A conditional WITHOUT calls yields no condition suggestion (pins the
    // condition branch's `&& has_calls`).
    assert!(
        suggestion_messages(&violation("if", false))
            .iter()
            .all(|(h, _)| !h.contains("Extract the condition")),
        "conditional without calls → no condition suggestion"
    );
    // A loop WITHOUT calls yields no iterator suggestion (pins the loop
    // branch's `&& has_calls`).
    assert!(
        suggestion_messages(&violation("for", false))
            .iter()
            .all(|(h, _)| !h.contains("iterator chain")),
        "loop without calls → no iterator suggestion"
    );
    // A conditional (has_conditional = true) suppresses the generic
    // extract-logic line — pins the `!has_conditional && !has_loop`.
    assert!(
        suggestion_messages(&violation("if", true))
            .iter()
            .all(|(h, _)| !h.contains("Extract the logic")),
        "a conditional present → no extract-logic suggestion"
    );
}

#[test]
fn collect_suggestions_filters_to_unsuppressed_violations() {
    let mut suppressed = violation("if", true);
    suppressed.qualified_name = "supp_fn".into();
    suppressed.suppressed = true;
    let mut live = violation("if", true);
    live.qualified_name = "live_fn".into();
    let results = vec![
        live,
        make_result("op", Classification::Operation),
        suppressed,
    ];
    let suggestions = collect_suggestions(&results);
    // Only the unsuppressed violation survives; pins `!suppressed` (a deleted `!`
    // would keep the suppressed one and drop the live one).
    assert_eq!(suggestions.len(), 1, "only the unsuppressed violation");
    assert_eq!(suggestions[0].qualified_name, "live_fn");
}
