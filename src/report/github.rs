use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};

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
    m: &crate::adapters::analyzers::iosp::ComplexityMetrics,
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
    let build = |func: &FunctionAnalysis,
                 m: &crate::adapters::analyzers::iosp::ComplexityMetrics| {
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
            summary.quality_score * crate::adapters::analyzers::iosp::PERCENTAGE_MULTIPLIER,
        );
    } else {
        println!(
            "::notice::Quality score: {:.1}% ({} functions analyzed)",
            summary.quality_score * crate::adapters::analyzers::iosp::PERCENTAGE_MULTIPLIER,
            summary.total,
        );
    }
}

// Additional annotation functions are in super::github_annotations.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::analyzers::iosp::{compute_severity, CallOccurrence, LogicOccurrence};
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
