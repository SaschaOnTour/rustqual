/// Collect SARIF result entries for test quality findings (TQ-001 through TQ-005).
/// Operation: iteration + match on kind + JSON construction, no own calls.
pub(super) fn collect_tq_findings(tq: &crate::tq::TqAnalysis) -> Vec<serde_json::Value> {
    tq.warnings
        .iter()
        .filter(|w| !w.suppressed)
        .map(|w| {
            let (rule_id, msg) = match &w.kind {
                crate::tq::TqWarningKind::NoAssertion => (
                    "TQ-001",
                    format!("Test '{}' has no assertions", w.function_name),
                ),
                crate::tq::TqWarningKind::NoSut => (
                    "TQ-002",
                    format!(
                        "Test '{}' does not call any production function",
                        w.function_name,
                    ),
                ),
                crate::tq::TqWarningKind::Untested => (
                    "TQ-003",
                    format!("Production function '{}' is untested", w.function_name),
                ),
                crate::tq::TqWarningKind::Uncovered => (
                    "TQ-004",
                    format!("Production function '{}' has no coverage", w.function_name,),
                ),
                crate::tq::TqWarningKind::UntestedLogic { uncovered_lines } => {
                    let lines: Vec<String> = uncovered_lines
                        .iter()
                        .map(|(f, l)| format!("{f}:{l}"))
                        .collect();
                    (
                        "TQ-005",
                        format!(
                            "Untested logic in '{}' at {}",
                            w.function_name,
                            lines.join(", "),
                        ),
                    )
                }
            };
            serde_json::json!({
                "ruleId": rule_id,
                "level": "warning",
                "message": { "text": msg },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": w.file },
                        "region": { "startLine": w.line }
                    }
                }]
            })
        })
        .collect()
}
