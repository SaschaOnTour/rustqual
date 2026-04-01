use crate::structural::StructuralAnalysis;

/// Collect SARIF result entries for structural check findings (BTC/SLM/NMS/OI/SIT/DEH/IET).
/// Operation: iteration + method calls via closure + JSON construction, no own calls.
pub(super) fn collect_structural_findings(
    structural: &StructuralAnalysis,
) -> Vec<serde_json::Value> {
    structural
        .warnings
        .iter()
        .filter(|w| !w.suppressed)
        .map(|w| {
            let rule_id = w.kind.code();
            let msg = format!("{}: '{}' — {}", rule_id, w.name, w.kind.detail());
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
