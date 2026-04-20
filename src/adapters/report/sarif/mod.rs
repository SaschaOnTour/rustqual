mod collectors;
mod rules;
mod structural_collector;
mod tq_collector;

use super::AnalysisResult;
use collectors::{
    collect_complexity_findings, collect_coupling_findings, collect_dry_findings,
    collect_param_srp_findings, collect_srp_findings, collect_violation_findings,
    collect_wildcard_findings,
};
use structural_collector::collect_structural_findings;
use tq_collector::collect_tq_findings;

/// Print results in SARIF v2.1.0 format for GitHub Code Scanning integration.
/// Trivial: delegates to build_sarif_value + stdout rendering.
pub fn print_sarif(analysis: &AnalysisResult) {
    let sarif = build_sarif_value(analysis);
    let rendered = serde_json::to_string_pretty(&sarif)
        .unwrap_or_else(|e| format!("{{\"error\":\"SARIF serialization failed: {e}\"}}"));
    println!("{rendered}");
}

/// Build the SARIF v2.1.0 JSON value from an analysis result. Exposed
/// so tests can assert on the exact output without capturing stdout.
/// Integration: orchestrates finding collection and envelope construction.
pub fn build_sarif_value(analysis: &AnalysisResult) -> serde_json::Value {
    let sarif_results = collect_all_findings(analysis);
    build_sarif_envelope(sarif_results)
}

/// Gather every SARIF result entry for an analysis run.
/// Integration: delegates per-dimension collection.
fn collect_all_findings(analysis: &AnalysisResult) -> Vec<serde_json::Value> {
    let mut sarif_results = collect_violation_findings(&analysis.results);
    sarif_results.extend(collect_complexity_findings(&analysis.results));
    sarif_results.extend(collect_extended_complexity_findings(&analysis.results));
    analysis
        .coupling
        .iter()
        .for_each(|ca| sarif_results.extend(collect_coupling_findings(ca)));
    sarif_results.extend(collect_dry_findings(
        &analysis.duplicates,
        &analysis.dead_code,
        &analysis.fragments,
        &analysis.boilerplate,
    ));
    sarif_results.extend(collect_wildcard_findings(&analysis.wildcard_warnings));
    analysis
        .coupling
        .iter()
        .for_each(|ca| sarif_results.extend(collect_sdp_findings(ca)));
    analysis
        .srp
        .iter()
        .for_each(|s| sarif_results.extend(collect_srp_findings(s)));
    analysis
        .srp
        .iter()
        .for_each(|s| sarif_results.extend(collect_param_srp_findings(s)));
    analysis
        .tq
        .iter()
        .for_each(|tq| sarif_results.extend(collect_tq_findings(tq)));
    analysis
        .structural
        .iter()
        .for_each(|s| sarif_results.extend(collect_structural_findings(s)));
    sarif_results.extend(collect_repeated_match_findings(&analysis.repeated_matches));
    sarif_results.extend(collect_orphan_suppression_findings(
        &analysis.orphan_suppressions,
    ));
    sarif_results.extend(collect_suppression_ratio_finding(&analysis.summary));
    sarif_results
}

/// Construct the SARIF envelope with tool metadata and results.
/// Operation: JSON construction logic, no own calls (rules via closure).
fn build_sarif_envelope(sarif_results: Vec<serde_json::Value>) -> serde_json::Value {
    let get_rules = || rules::sarif_rules();
    serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "rustqual",
                    "informationUri": "https://github.com/DEIN-USERNAME/rustqual",
                    "rules": get_rules()
                }
            },
            "results": sarif_results,
        }]
    })
}

/// Collect SARIF result entries for orphan-suppression findings.
/// Operation: iteration + JSON construction.
fn collect_orphan_suppression_findings(
    orphans: &[crate::adapters::report::OrphanSuppressionWarning],
) -> Vec<serde_json::Value> {
    orphans
        .iter()
        .map(|w| {
            let dims: String = if w.dimensions.is_empty() {
                "all dims (wildcard)".to_string()
            } else {
                w.dimensions
                    .iter()
                    .map(|d| format!("{d}"))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let message = match &w.reason {
                Some(r) => format!("Stale qual:allow({dims}) marker — no finding in window. Reason was: {r}"),
                None => format!("Stale qual:allow({dims}) marker — no finding in window."),
            };
            serde_json::json!({
                "ruleId": "ORPHAN-001",
                "level": "warning",
                "message": { "text": message },
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

/// Collect SARIF result entries for SDP violations, skipping suppressed ones.
/// Operation: iteration + JSON construction.
fn collect_sdp_findings(
    analysis: &crate::adapters::analyzers::coupling::CouplingAnalysis,
) -> Vec<serde_json::Value> {
    analysis
        .sdp_violations
        .iter()
        .filter(|v| !v.suppressed)
        .map(|v| {
            serde_json::json!({
                "ruleId": "CP-002",
                "level": "warning",
                "message": {
                    "text": format!(
                        "SDP violation: '{}' (I={:.2}) depends on '{}' (I={:.2})",
                        v.from_module, v.from_instability,
                        v.to_module, v.to_instability,
                    )
                },
                "locations": []
            })
        })
        .collect()
}

/// Build SARIF entries for a single function's extended complexity warnings.
/// Operation: data-driven array + JSON construction, no own calls.
fn build_extended_entries(
    func: &crate::adapters::analyzers::iosp::FunctionAnalysis,
    m: &crate::adapters::analyzers::iosp::ComplexityMetrics,
) -> Vec<serde_json::Value> {
    let finding = |rule: &str, level: &str, msg: String| -> serde_json::Value {
        serde_json::json!({
            "ruleId": rule, "level": level,
            "message": { "text": msg },
            "locations": [{"physicalLocation": {
                "artifactLocation": { "uri": &func.file },
                "region": { "startLine": func.line }
            }}]
        })
    };
    let err_msg = func.error_handling_warning.then(|| {
        let parts: Vec<String> = [
            (m.unwrap_count, "unwrap"),
            (m.expect_count, "expect"),
            (m.panic_count, "panic"),
            (m.todo_count, "todo"),
        ]
        .iter()
        .filter(|(c, _)| *c > 0)
        .map(|(c, l)| format!("{c} {l}"))
        .collect();
        format!(
            "Error handling in {}: {}",
            func.qualified_name,
            parts.join(", ")
        )
    });
    [
        func.function_length_warning.then(|| {
            (
                "CX-004",
                "warning",
                format!(
                    "Function {} has {} lines (exceeds threshold)",
                    func.qualified_name, m.function_lines
                ),
            )
        }),
        func.nesting_depth_warning.then(|| {
            (
                "CX-005",
                "warning",
                format!(
                    "Nesting depth {} in {} exceeds threshold",
                    m.max_nesting, func.qualified_name
                ),
            )
        }),
        func.unsafe_warning.then(|| {
            (
                "CX-006",
                "warning",
                format!(
                    "{} unsafe block(s) in {}",
                    m.unsafe_blocks, func.qualified_name
                ),
            )
        }),
        err_msg.map(|msg| ("A20", "warning", msg)),
    ]
    .into_iter()
    .flatten()
    .map(|(rule, level, msg)| finding(rule, level, msg))
    .collect()
}

/// Collect SARIF result entries for extended complexity warnings (CX-004/005/006/A20).
/// Operation: iteration + helper call via closure, no direct own calls.
fn collect_extended_complexity_findings(
    results: &[crate::adapters::analyzers::iosp::FunctionAnalysis],
) -> Vec<serde_json::Value> {
    let build = |func: &crate::adapters::analyzers::iosp::FunctionAnalysis,
                 m: &crate::adapters::analyzers::iosp::ComplexityMetrics| {
        build_extended_entries(func, m)
    };
    let mut findings = Vec::new();
    for func in results {
        if func.suppressed || func.complexity_suppressed {
            continue;
        }
        if let Some(ref m) = func.complexity {
            findings.extend(build(func, m));
        }
    }
    findings
}

/// Collect SARIF result entries for repeated match pattern findings (DRY-005).
/// Operation: iteration + JSON construction.
fn collect_repeated_match_findings(
    groups: &[crate::adapters::analyzers::dry::match_patterns::RepeatedMatchGroup],
) -> Vec<serde_json::Value> {
    groups
        .iter()
        .filter(|g| !g.suppressed)
        .flat_map(|g| {
            g.entries.iter().map(move |e| {
                serde_json::json!({
                    "ruleId": "DRY-005",
                    "level": "note",
                    "message": {
                        "text": format!(
                            "Repeated match on '{}' ({} arms) in {}",
                            g.enum_name, e.arm_count, e.function_name,
                        )
                    },
                    "locations": [{
                        "physicalLocation": {
                            "artifactLocation": { "uri": e.file },
                            "region": { "startLine": e.line }
                        }
                    }]
                })
            })
        })
        .collect()
}

/// Collect a SARIF notification if the suppression ratio is exceeded.
/// Operation: conditional JSON construction.
fn collect_suppression_ratio_finding(summary: &crate::report::Summary) -> Vec<serde_json::Value> {
    if !summary.suppression_ratio_exceeded {
        return vec![];
    }
    vec![serde_json::json!({
        "ruleId": "SUP-001",
        "level": "note",
        "message": {
            "text": format!(
                "Suppression ratio exceeded: {} suppressions (qual:allow + #[allow]) of {} functions",
                summary.all_suppressions, summary.total,
            )
        },
        "locations": []
    })]
}

#[cfg(test)]
mod tests;
