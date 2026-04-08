use crate::analyzer::{Classification, FunctionAnalysis, Severity};

/// Collect SARIF result entries for IOSP violations.
/// Operation: iteration + classification matching + JSON construction.
pub(super) fn collect_violation_findings(results: &[FunctionAnalysis]) -> Vec<serde_json::Value> {
    let mut findings = Vec::new();
    for func in results {
        if func.suppressed {
            continue;
        }
        if let Classification::Violation {
            logic_locations,
            call_locations,
            ..
        } = &func.classification
        {
            let logic_desc: Vec<String> = logic_locations.iter().map(|l| l.to_string()).collect();
            let call_desc: Vec<String> = call_locations.iter().map(|c| c.to_string()).collect();

            let level = match &func.severity {
                Some(Severity::High) => "error",
                Some(Severity::Medium) => "warning",
                _ => "note",
            };

            let effort_tag = func
                .effort_score
                .map(|e| format!(" (effort: {e:.1})"))
                .unwrap_or_default();
            findings.push(serde_json::json!({
                "ruleId": "iosp/violation",
                "level": level,
                "message": {
                    "text": format!(
                        "IOSP violation in {qname}: mixes logic [{logic}] with own calls [{calls}]{effort}",
                        qname = func.qualified_name,
                        logic = logic_desc.join(", "),
                        calls = call_desc.join(", "),
                        effort = effort_tag,
                    )
                },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": func.file },
                        "region": { "startLine": func.line }
                    }
                }]
            }));
        }
    }
    findings
}

/// Collect SARIF result entries for complexity warnings.
/// Operation: iteration + conditional logic + JSON construction.
pub(super) fn collect_complexity_findings(results: &[FunctionAnalysis]) -> Vec<serde_json::Value> {
    let mut findings = Vec::new();
    for func in results {
        if func.suppressed {
            continue;
        }
        if let Some(ref m) = func.complexity {
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
            if func.cognitive_warning {
                findings.push(finding(
                    "CX-001",
                    "warning",
                    format!(
                        "Cognitive complexity {} in {} exceeds threshold",
                        m.cognitive_complexity, func.qualified_name,
                    ),
                ));
            }
            if func.cyclomatic_warning {
                findings.push(finding(
                    "CX-002",
                    "warning",
                    format!(
                        "Cyclomatic complexity {} in {} exceeds threshold",
                        m.cyclomatic_complexity, func.qualified_name,
                    ),
                ));
            }
            if !m.magic_numbers.is_empty() {
                let nums: Vec<String> = m.magic_numbers.iter().map(|n| n.value.clone()).collect();
                findings.push(finding(
                    "CX-003",
                    "note",
                    format!(
                        "Magic numbers in {}: {}",
                        func.qualified_name,
                        nums.join(", "),
                    ),
                ));
            }
        }
    }
    findings
}

/// Collect SARIF result entries for coupling issues.
/// Operation: iteration + JSON construction.
pub(super) fn collect_coupling_findings(
    analysis: &crate::coupling::CouplingAnalysis,
) -> Vec<serde_json::Value> {
    let mut findings = Vec::new();
    for cycle in &analysis.cycles {
        findings.push(serde_json::json!({
            "ruleId": "CP-001",
            "level": "error",
            "message": {
                "text": format!(
                    "Circular module dependency: {}",
                    cycle.modules.join(" → "),
                )
            },
            "locations": []
        }));
    }
    findings
}

/// Collect SARIF result entries for DRY findings (duplicates, dead code, fragments, boilerplate).
/// Operation: iteration + JSON construction.
pub(super) fn collect_dry_findings(
    duplicates: &[crate::dry::DuplicateGroup],
    dead_code: &[crate::dry::DeadCodeWarning],
    fragments: &[crate::dry::FragmentGroup],
    boilerplate: &[crate::dry::BoilerplateFind],
) -> Vec<serde_json::Value> {
    let mut findings = Vec::new();
    let finding = |rule: &str, level: &str, msg: String, file: &str, line: usize| {
        serde_json::json!({
            "ruleId": rule, "level": level,
            "message": { "text": msg },
            "locations": [{"physicalLocation": {
                "artifactLocation": { "uri": file },
                "region": { "startLine": line }
            }}]
        })
    };
    for g in duplicates.iter().filter(|g| !g.suppressed) {
        let names: Vec<&str> = g
            .entries
            .iter()
            .map(|e| e.qualified_name.as_str())
            .collect();
        let msg = format!("Duplicate function group: {}", names.join(", "));
        g.entries.iter().for_each(|e| {
            findings.push(finding("DRY-001", "warning", msg.clone(), &e.file, e.line));
        });
    }
    dead_code.iter().for_each(|w| {
        findings.push(finding(
            "DRY-002",
            "note",
            format!("{}: {}", w.qualified_name, w.suggestion),
            &w.file,
            w.line,
        ));
    });
    for g in fragments {
        g.entries.iter().for_each(|e| {
            findings.push(finding(
                "DRY-003",
                "note",
                format!(
                    "Duplicate fragment ({} stmts) in {}",
                    g.statement_count, e.qualified_name
                ),
                &e.file,
                e.start_line,
            ));
        });
    }
    boilerplate.iter().for_each(|b| {
        findings.push(finding(
            &format!("BP-{}", &b.pattern_id),
            "note",
            format!("{} — {}", b.description, b.suggestion),
            &b.file,
            b.line,
        ));
    });
    findings
}

/// Collect SARIF result entries for SRP findings.
/// Operation: iteration + JSON construction.
pub(super) fn collect_srp_findings(srp: &crate::srp::SrpAnalysis) -> Vec<serde_json::Value> {
    let mut findings = Vec::new();
    for w in &srp.struct_warnings {
        if w.suppressed {
            continue;
        }
        findings.push(serde_json::json!({
            "ruleId": "SRP-001",
            "level": "warning",
            "message": {
                "text": format!(
                    "Struct '{}' may violate SRP: LCOM4={}, score={:.2}",
                    w.struct_name, w.lcom4, w.composite_score,
                )
            },
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": { "uri": w.file },
                    "region": { "startLine": w.line }
                }
            }]
        }));
    }
    for w in &srp.module_warnings {
        if w.suppressed {
            continue;
        }
        let mut parts = Vec::new();
        if w.length_score > 0.0 {
            parts.push(format!(
                "{} production lines (score={:.2})",
                w.production_lines, w.length_score,
            ));
        }
        if w.independent_clusters > 0 {
            parts.push(format!(
                "{} independent function clusters",
                w.independent_clusters,
            ));
        }
        let text = format!("Module '{}': {}", w.module, parts.join(", "));
        findings.push(serde_json::json!({
            "ruleId": "SRP-002",
            "level": "note",
            "message": { "text": text },
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": { "uri": w.file },
                    "region": { "startLine": 1 }
                }
            }]
        }));
    }
    findings
}

/// Collect SARIF result entries for too-many-arguments SRP findings.
/// Operation: iteration + JSON construction.
pub(super) fn collect_param_srp_findings(srp: &crate::srp::SrpAnalysis) -> Vec<serde_json::Value> {
    srp.param_warnings
        .iter()
        .filter(|w| !w.suppressed)
        .map(|w| {
            serde_json::json!({
                "ruleId": "SRP-003",
                "level": "warning",
                "message": {
                    "text": format!(
                        "Function '{}' has {} parameters — reduce parameter count or restructure",
                        w.function_name,
                        w.parameter_count,
                    )
                },
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

/// Collect SARIF result entries for wildcard import warnings.
/// Operation: iteration + JSON construction.
pub(super) fn collect_wildcard_findings(
    warnings: &[crate::dry::wildcards::WildcardImportWarning],
) -> Vec<serde_json::Value> {
    warnings
        .iter()
        .filter(|w| !w.suppressed)
        .map(|w| {
            serde_json::json!({
                "ruleId": "DRY-004",
                "level": "note",
                "message": {
                    "text": format!("Wildcard import: {}", w.module_path)
                },
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
