use crate::analyzer::{Classification, FunctionAnalysis};

use super::Summary;

/// Print results as GitHub Actions workflow annotations.
/// Integration: orchestrates violation, complexity, and summary annotations.
pub fn print_github(results: &[FunctionAnalysis], summary: &Summary) {
    print_violation_annotations(results);
    print_complexity_annotations(results);
    print_summary_annotation(summary);
}

/// Print `::warning` annotations for IOSP violations.
/// Operation: iteration + classification matching logic, no own calls.
fn print_violation_annotations(results: &[FunctionAnalysis]) {
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

            let effort_tag = func
                .effort_score
                .map(|e| format!(", effort={e:.1}"))
                .unwrap_or_default();
            println!(
                "::warning file={},line={}::IOSP violation in {}: logic=[{}], calls=[{}]{}",
                func.file,
                func.line,
                func.qualified_name,
                logic_desc.join(", "),
                call_desc.join(", "),
                effort_tag,
            );
        }
    }
}

/// Build annotation pairs (level, message) for a single function's complexity.
/// Operation: data-driven array construction, no own calls.
fn build_annotation_pairs(
    func: &FunctionAnalysis,
    m: &crate::analyzer::ComplexityMetrics,
) -> Vec<(&'static str, String)> {
    let q = &func.qualified_name;
    let magic_msg = (!m.magic_numbers.is_empty()).then(|| {
        let nums: Vec<String> = m.magic_numbers.iter().map(|n| n.value.clone()).collect();
        format!("Magic numbers in {q}: {}", nums.join(", "))
    });
    [
        func.cognitive_warning.then(|| {
            (
                "notice",
                format!(
                    "Cognitive complexity {} in {q} exceeds threshold",
                    m.cognitive_complexity
                ),
            )
        }),
        func.cyclomatic_warning.then(|| {
            (
                "notice",
                format!(
                    "Cyclomatic complexity {} in {q} exceeds threshold",
                    m.cyclomatic_complexity
                ),
            )
        }),
        magic_msg.map(|msg| ("warning", msg)),
        func.nesting_depth_warning.then(|| {
            (
                "notice",
                format!("Nesting depth {} in {q} exceeds threshold", m.max_nesting),
            )
        }),
        func.function_length_warning.then(|| {
            (
                "notice",
                format!(
                    "Function {q} has {} lines (exceeds threshold)",
                    m.function_lines
                ),
            )
        }),
        func.unsafe_warning.then(|| {
            (
                "warning",
                format!("{} unsafe block(s) in {q}", m.unsafe_blocks),
            )
        }),
        func.error_handling_warning.then(|| {
            (
                "warning",
                format!(
                    "Error handling in {q}: unwrap={}, expect={}, panic={}, todo={}",
                    m.unwrap_count, m.expect_count, m.panic_count, m.todo_count,
                ),
            )
        }),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Print `::notice`/`::warning` annotations for complexity findings.
/// Operation: iteration + helper call via closure, no direct own calls.
fn print_complexity_annotations(results: &[FunctionAnalysis]) {
    let build = |func: &FunctionAnalysis, m: &crate::analyzer::ComplexityMetrics| {
        build_annotation_pairs(func, m)
    };
    for func in results {
        if func.suppressed {
            continue;
        }
        let Some(ref m) = func.complexity else {
            continue;
        };
        let (f, l) = (&func.file, func.line);
        build(func, m).iter().for_each(|(level, msg)| {
            println!("::{level} file={f},line={l}::{msg}");
        });
    }
}

/// Print `::error` or `::notice` summary annotation.
/// Operation: conditional formatting logic, no own calls.
fn print_summary_annotation(summary: &Summary) {
    if summary.suppression_ratio_exceeded {
        println!(
            "::warning::Suppression ratio exceeds configured maximum ({} of {} functions suppressed)",
            summary.suppressed,
            summary.total,
        );
    }
    if summary.violations > 0 {
        println!(
            "::error::Quality analysis: {} violation(s), {:.1}% quality score",
            summary.violations,
            summary.quality_score * crate::analyzer::PERCENTAGE_MULTIPLIER,
        );
    } else {
        println!(
            "::notice::Quality score: {:.1}% ({} functions analyzed)",
            summary.quality_score * crate::analyzer::PERCENTAGE_MULTIPLIER,
            summary.total,
        );
    }
}

/// Print `::notice` annotations for DRY/boilerplate/wildcard/repeated-match findings.
/// Operation: iteration + formatting logic, no own calls.
pub fn print_dry_annotations(analysis: &super::AnalysisResult) {
    for g in &analysis.duplicates {
        let names: Vec<&str> = g
            .entries
            .iter()
            .map(|e| e.qualified_name.as_str())
            .collect();
        println!("::notice::Duplicate functions: {}", names.join(", "),);
    }
    for w in &analysis.dead_code {
        println!(
            "::notice file={},line={}::Dead code: {} — {}",
            w.file, w.line, w.qualified_name, w.suggestion,
        );
    }
    for g in &analysis.fragments {
        let names: Vec<&str> = g
            .entries
            .iter()
            .map(|e| e.qualified_name.as_str())
            .collect();
        println!(
            "::notice::Duplicate fragment ({} stmts): {}",
            g.statement_count,
            names.join(", "),
        );
    }
    for b in &analysis.boilerplate {
        println!(
            "::notice file={},line={}::{} — {}",
            b.file, b.line, b.description, b.suggestion,
        );
    }
    for w in &analysis.wildcard_warnings {
        if w.suppressed {
            continue;
        }
        println!(
            "::notice file={},line={}::Wildcard import: {}",
            w.file, w.line, w.module_path,
        );
    }
    for g in &analysis.repeated_matches {
        let fns: Vec<&str> = g.entries.iter().map(|e| e.function_name.as_str()).collect();
        println!(
            "::notice::DRY-005: Repeated match on '{}' in: {}",
            g.enum_name,
            fns.join(", "),
        );
    }
}

/// Print `::error` annotations for circular module dependencies.
/// Operation: iteration + formatting logic, no own calls.
/// Leaf modules (afferent=0) are excluded from instability warnings.
pub fn print_coupling_annotations(
    analysis: &crate::coupling::CouplingAnalysis,
    config: &crate::config::sections::CouplingConfig,
) {
    for cycle in &analysis.cycles {
        println!(
            "::error::Circular module dependency: {}",
            cycle.modules.join(" → "),
        );
    }
    for m in &analysis.metrics {
        if m.suppressed {
            continue;
        }
        if m.afferent > 0 && m.instability > config.max_instability {
            println!(
                "::warning::Module '{}' has high instability ({:.2})",
                m.module_name, m.instability,
            );
        }
    }
    for v in &analysis.sdp_violations {
        if v.suppressed {
            continue;
        }
        println!(
            "::warning::SDP violation: '{}' (I={:.2}) depends on '{}' (I={:.2})",
            v.from_module, v.from_instability, v.to_module, v.to_instability,
        );
    }
}

/// Print `::warning` annotations for TQ findings.
/// Operation: iteration + formatting logic, no own calls.
pub fn print_tq_annotations(tq: &crate::tq::TqAnalysis) {
    for w in &tq.warnings {
        if w.suppressed {
            continue;
        }
        let kind_label = match &w.kind {
            crate::tq::TqWarningKind::NoAssertion => "TQ-001: test has no assertions".to_string(),
            crate::tq::TqWarningKind::NoSut => {
                "TQ-002: test does not call production code".to_string()
            }
            crate::tq::TqWarningKind::Untested => {
                "TQ-003: production function is untested".to_string()
            }
            crate::tq::TqWarningKind::Uncovered => {
                "TQ-004: production function has no coverage".to_string()
            }
            crate::tq::TqWarningKind::UntestedLogic { uncovered_lines } => {
                let lines: Vec<String> = uncovered_lines
                    .iter()
                    .map(|(f, l)| format!("{f}:{l}"))
                    .collect();
                format!("TQ-005: untested logic at {}", lines.join(", "))
            }
        };
        println!(
            "::warning file={},line={}::{} in '{}'",
            w.file, w.line, kind_label, w.function_name,
        );
    }
}

/// Print `::warning` annotations for structural findings.
/// Trivial: iteration with method calls hidden in closure (lenient mode).
pub fn print_structural_annotations(structural: &crate::structural::StructuralAnalysis) {
    structural
        .warnings
        .iter()
        .filter(|w| !w.suppressed)
        .for_each(|w| {
            let (code, detail) = (w.kind.code(), w.kind.detail());
            println!(
                "::warning file={},line={}::{code}: '{}' — {detail}",
                w.file, w.line, w.name,
            );
        });
}

/// Print `::warning` annotations for SRP findings.
/// Operation: iteration + formatting logic, no own calls.
pub fn print_srp_annotations(srp: &crate::srp::SrpAnalysis) {
    for w in &srp.struct_warnings {
        if w.suppressed {
            continue;
        }
        println!(
            "::warning file={},line={}::SRP warning: {} has LCOM4={}, score={:.2}",
            w.file, w.line, w.struct_name, w.lcom4, w.composite_score,
        );
    }
    for w in &srp.module_warnings {
        if w.suppressed {
            continue;
        }
        if w.length_score > 0.0 {
            println!(
                "::warning file={}::Module has {} production lines (score={:.2})",
                w.file, w.production_lines, w.length_score,
            );
        }
        if w.independent_clusters > 0 {
            println!(
                "::warning file={}::Module has {} independent function clusters",
                w.file, w.independent_clusters,
            );
        }
    }
    for w in &srp.param_warnings {
        if w.suppressed {
            continue;
        }
        println!(
            "::warning file={},line={}::Function '{}' has {} parameters — reduce parameter count",
            w.file, w.line, w.function_name, w.parameter_count,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::{compute_severity, CallOccurrence, LogicOccurrence};
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

    #[test]
    fn test_print_github_no_violations_no_panic() {
        let results = vec![make_result("good_fn", Classification::Integration)];
        let summary = Summary::from_results(&results);
        print_github(&results, &summary);
    }

    #[test]
    fn test_print_github_with_violation_no_panic() {
        let results = vec![make_result(
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
        )];
        let summary = Summary::from_results(&results);
        print_github(&results, &summary);
    }

    #[test]
    fn test_print_github_suppressed_skipped() {
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
        let summary = Summary::from_results(&results);
        print_github(&results, &summary);
    }

    #[test]
    fn test_print_github_multiple_violations() {
        let results = vec![
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
                        kind: "match".into(),
                        line: 10,
                    }],
                    call_locations: vec![CallOccurrence {
                        name: "b".into(),
                        line: 12,
                    }],
                },
            ),
        ];
        let summary = Summary::from_results(&results);
        print_github(&results, &summary);
    }
}
