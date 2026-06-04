//! JSON per-function violation-location tests. `violation_locations` attaches
//! an IOSP finding's logic/call locations to a function record only when both
//! file AND line match — pins the `file == … && line == …` lookup.
use super::*;
use crate::domain::analysis_data::{FunctionClassification, FunctionRecord};
use crate::domain::findings::{IospFinding, LogicLocation};
use crate::domain::{AnalysisData, AnalysisFindings, Dimension, Finding, Severity};
use crate::report::{AnalysisResult, Summary};

fn finding_at(file: &str, line: usize) -> Finding {
    Finding {
        file: file.into(),
        line,
        column: 0,
        dimension: Dimension::Iosp,
        rule_id: "iosp/violation".into(),
        message: "m".into(),
        severity: Severity::High,
        suppressed: false,
    }
}

fn violation_record(file: &str, line: usize, name: &str) -> FunctionRecord {
    FunctionRecord {
        name: name.into(),
        file: file.into(),
        line,
        qualified_name: name.into(),
        parent_type: None,
        classification: FunctionClassification::Violation,
        severity: Some(Severity::High),
        complexity: None,
        parameter_count: 0,
        own_calls: vec![],
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
        suppressed: false,
        complexity_suppressed: false,
    }
}

fn function_entry(func_line: usize, iosp_line: usize) -> serde_json::Value {
    let iosp = IospFinding {
        common: finding_at("test.rs", iosp_line),
        logic_locations: vec![LogicLocation {
            kind: "if".into(),
            line: 1,
        }],
        call_locations: vec![],
        effort_score: None,
    };
    let analysis = AnalysisResult {
        results: vec![],
        summary: Summary::default(),
        findings: AnalysisFindings {
            iosp: vec![iosp],
            ..Default::default()
        },
        data: AnalysisData {
            functions: vec![violation_record("test.rs", func_line, "viol")],
            ..Default::default()
        },
    };
    function_named(&json_value(&analysis), "viol").clone()
}

#[test]
fn violation_locations_attach_only_on_exact_line_match() {
    // Same file + same line → the IOSP finding's logic location is attached.
    let matched = function_entry(5, 5);
    assert_eq!(
        matched["logic"].as_array().map(Vec::len),
        Some(1),
        "matching file+line attaches logic: {matched}"
    );
    // Same file but a different line → no attachment (pins the `&&`).
    let mismatched = function_entry(5, 99);
    assert!(
        mismatched["logic"].as_array().is_none_or(|a| a.is_empty()),
        "line mismatch → no logic locations: {mismatched}"
    );
}
