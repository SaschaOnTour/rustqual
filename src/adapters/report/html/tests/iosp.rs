//! HTML IOSP section: the violation-row filter (`!suppressed && classification
//! == Violation`) and the finding↔function join (`file == … && line == …`).
use crate::domain::analysis_data::{FunctionClassification, FunctionRecord};
use crate::report::html::iosp::{build_iosp_data_view, format_iosp_section};
use crate::report::html::views::{HtmlIospFindingRow, HtmlIospView};
use crate::report::Summary;

fn record(classification: FunctionClassification, suppressed: bool) -> FunctionRecord {
    FunctionRecord {
        name: "f".into(),
        file: "a.rs".into(),
        line: 5,
        qualified_name: "viol_fn".into(),
        parent_type: None,
        classification,
        severity: Some(crate::domain::Severity::High),
        complexity: None,
        parameter_count: 0,
        own_calls: vec![],
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
        suppressed,
        complexity_suppressed: false,
    }
}

#[test]
fn build_iosp_data_view_keeps_only_unsuppressed_violations() {
    // Operation (non-violation) and suppressed violation rows are dropped; pins
    // `!suppressed && classification == Violation`.
    assert!(
        build_iosp_data_view(&[record(FunctionClassification::Operation, false)])
            .violations
            .is_empty(),
        "non-violation excluded"
    );
    assert!(
        build_iosp_data_view(&[record(FunctionClassification::Violation, true)])
            .violations
            .is_empty(),
        "suppressed violation excluded"
    );
    assert_eq!(
        build_iosp_data_view(&[record(FunctionClassification::Violation, false)])
            .violations
            .len(),
        1,
        "unsuppressed violation kept"
    );
}

#[test]
fn iosp_section_join_requires_exact_file_and_line() {
    // The violation row is at a.rs:5; a finding at a.rs:99 must NOT join (pins
    // the `file == … && line == …` match), leaving the logic/calls cells empty.
    let data = build_iosp_data_view(&[record(FunctionClassification::Violation, false)]);
    let findings = HtmlIospView {
        findings: vec![HtmlIospFindingRow {
            file: "a.rs".into(),
            line: 99,
            logic_summary: "LOGIC_MARKER".into(),
            call_summary: "CALL_MARKER".into(),
        }],
    };
    let summary = Summary {
        violations: 1,
        ..Default::default()
    };
    let html = format_iosp_section(&findings, &data, &summary);
    assert!(html.contains("viol_fn"), "violation row rendered: {html}");
    assert!(
        !html.contains("LOGIC_MARKER"),
        "line mismatch → finding does not join: {html}"
    );
}
