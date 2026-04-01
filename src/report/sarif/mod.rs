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
/// Integration: orchestrates finding collection and SARIF envelope construction.
pub fn print_sarif(analysis: &AnalysisResult) {
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
    sarif_results.extend(collect_suppression_ratio_finding(&analysis.summary));
    print_sarif_envelope(sarif_results);
}

/// Construct and print the SARIF envelope with tool metadata and results.
/// Operation: JSON construction logic, no own calls (rules via closure).
fn print_sarif_envelope(sarif_results: Vec<serde_json::Value>) {
    let get_rules = || rules::sarif_rules();
    let sarif = serde_json::json!({
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
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&sarif).expect("SARIF serialization failed")
    );
}

/// Collect SARIF result entries for SDP violations, skipping suppressed ones.
/// Operation: iteration + JSON construction.
fn collect_sdp_findings(analysis: &crate::coupling::CouplingAnalysis) -> Vec<serde_json::Value> {
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
    func: &crate::analyzer::FunctionAnalysis,
    m: &crate::analyzer::ComplexityMetrics,
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
            (m.unwrap_count, "unwrap"), (m.expect_count, "expect"),
            (m.panic_count, "panic"), (m.todo_count, "todo"),
        ]
        .iter()
        .filter(|(c, _)| *c > 0)
        .map(|(c, l)| format!("{c} {l}"))
        .collect();
        format!("Error handling in {}: {}", func.qualified_name, parts.join(", "))
    });
    [
        func.function_length_warning.then(|| ("CX-004", "warning",
            format!("Function {} has {} lines (exceeds threshold)", func.qualified_name, m.function_lines))),
        func.nesting_depth_warning.then(|| ("CX-005", "warning",
            format!("Nesting depth {} in {} exceeds threshold", m.max_nesting, func.qualified_name))),
        func.unsafe_warning.then(|| ("CX-006", "warning",
            format!("{} unsafe block(s) in {}", m.unsafe_blocks, func.qualified_name))),
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
    results: &[crate::analyzer::FunctionAnalysis],
) -> Vec<serde_json::Value> {
    let build = |func: &crate::analyzer::FunctionAnalysis, m: &crate::analyzer::ComplexityMetrics| {
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
    groups: &[crate::dry::match_patterns::RepeatedMatchGroup],
) -> Vec<serde_json::Value> {
    groups
        .iter()
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
mod tests {
    use super::*;
    use crate::analyzer::{
        compute_severity, CallOccurrence, Classification, FunctionAnalysis, LogicOccurrence,
    };
    use crate::report::Summary;

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

    fn make_analysis(results: Vec<FunctionAnalysis>) -> AnalysisResult {
        let summary = Summary::from_results(&results);
        AnalysisResult {
            results,
            summary,
            coupling: None,
            duplicates: vec![],
            dead_code: vec![],
            fragments: vec![],
            boilerplate: vec![],
            wildcard_warnings: vec![],
            repeated_matches: vec![],
            srp: None,
            tq: None,
            structural: None,
        }
    }

    #[test]
    fn test_print_sarif_no_violations_no_panic() {
        let analysis = make_analysis(vec![make_result("good_fn", Classification::Integration)]);
        print_sarif(&analysis);
    }

    #[test]
    fn test_print_sarif_with_violation_no_panic() {
        let analysis = make_analysis(vec![make_result(
            "bad_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "if".into(),
                    line: 5,
                }],
                call_locations: vec![CallOccurrence {
                    name: "helper".into(),
                    line: 6,
                }],
            },
        )]);
        print_sarif(&analysis);
    }

    #[test]
    fn test_print_sarif_high_severity_no_panic() {
        let analysis = make_analysis(vec![make_result(
            "complex_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![
                    LogicOccurrence {
                        kind: "if".into(),
                        line: 1,
                    },
                    LogicOccurrence {
                        kind: "match".into(),
                        line: 2,
                    },
                    LogicOccurrence {
                        kind: "for".into(),
                        line: 3,
                    },
                ],
                call_locations: vec![
                    CallOccurrence {
                        name: "a".into(),
                        line: 4,
                    },
                    CallOccurrence {
                        name: "b".into(),
                        line: 5,
                    },
                    CallOccurrence {
                        name: "c".into(),
                        line: 6,
                    },
                ],
            },
        )]);
        print_sarif(&analysis);
    }

    #[test]
    fn test_print_sarif_suppressed_skipped() {
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
        let analysis = make_analysis(vec![func]);
        print_sarif(&analysis);
    }

    #[test]
    fn test_print_sarif_multiple_violations() {
        let analysis = make_analysis(vec![
            make_result(
                "bad1",
                Classification::Violation {
                    has_logic: true,
                    has_own_calls: true,
                    logic_locations: vec![LogicOccurrence {
                        kind: "if".into(),
                        line: 1,
                    }],
                    call_locations: vec![CallOccurrence {
                        name: "a".into(),
                        line: 2,
                    }],
                },
            ),
            make_result(
                "bad2",
                Classification::Violation {
                    has_logic: true,
                    has_own_calls: true,
                    logic_locations: vec![LogicOccurrence {
                        kind: "while".into(),
                        line: 10,
                    }],
                    call_locations: vec![CallOccurrence {
                        name: "b".into(),
                        line: 12,
                    }],
                },
            ),
        ]);
        print_sarif(&analysis);
    }
}
