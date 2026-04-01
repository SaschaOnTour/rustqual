use std::collections::HashMap;

use crate::analyzer::FunctionAnalysis;

use super::lcov::LcovFileData;
use super::{TqWarning, TqWarningKind};

/// Detect production functions with 0 execution count in LCOV data (TQ-004).
/// Operation: iteration + lookup logic, no own calls.
pub(crate) fn detect_uncovered_functions(
    all_results: &[FunctionAnalysis],
    lcov_data: &HashMap<String, LcovFileData>,
) -> Vec<TqWarning> {
    all_results
        .iter()
        .filter(|fa| !fa.suppressed && !is_test_function(&fa.name))
        .filter_map(|fa| {
            let file_data = lcov_data.get(&fa.file)?;
            let hit_count = file_data.function_hits.get(&fa.name)?;
            if *hit_count == 0 {
                Some(TqWarning {
                    file: fa.file.clone(),
                    line: fa.line,
                    function_name: fa.name.clone(),
                    kind: TqWarningKind::Uncovered,
                    suppressed: false,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Detect logic occurrences at uncovered lines (TQ-005).
/// Operation: cross-references logic occurrences with LCOV line hits.
pub(crate) fn detect_untested_logic(
    all_results: &[FunctionAnalysis],
    lcov_data: &HashMap<String, LcovFileData>,
) -> Vec<TqWarning> {
    all_results
        .iter()
        .filter(|fa| !fa.suppressed && !is_test_function(&fa.name))
        .filter_map(|fa| {
            let file_data = lcov_data.get(&fa.file)?;
            let complexity = fa.complexity.as_ref()?;

            // Collect logic occurrence lines that are uncovered
            let uncovered: Vec<(String, usize)> = complexity
                .logic_occurrences
                .iter()
                .filter(|lo| {
                    file_data
                        .line_hits
                        .get(&lo.line)
                        .map(|&count| count == 0)
                        .unwrap_or(false) // skip if line not in LCOV
                })
                .map(|lo| (lo.kind.clone(), lo.line))
                .collect();

            if uncovered.is_empty() {
                return None;
            }

            Some(TqWarning {
                file: fa.file.clone(),
                line: fa.line,
                function_name: fa.name.clone(),
                kind: TqWarningKind::UntestedLogic {
                    uncovered_lines: uncovered,
                },
                suppressed: false,
            })
        })
        .collect()
}

/// Check if a function name looks like a test function.
/// Operation: string prefix check.
fn is_test_function(name: &str) -> bool {
    name.starts_with("test_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::{compute_severity, Classification, ComplexityMetrics, LogicOccurrence};

    fn make_func(name: &str, file: &str, line: usize) -> FunctionAnalysis {
        let severity = compute_severity(&Classification::Operation);
        FunctionAnalysis {
            name: name.to_string(),
            file: file.to_string(),
            line,
            classification: Classification::Operation,
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

    fn make_lcov_data(fn_hits: &[(&str, u64)], line_hits: &[(usize, u64)]) -> LcovFileData {
        LcovFileData {
            function_hits: fn_hits.iter().map(|(n, c)| (n.to_string(), *c)).collect(),
            line_hits: line_hits.iter().copied().collect(),
        }
    }

    // ── TQ-004 tests ────────────────────────────────────────

    #[test]
    fn test_uncovered_function_detected() {
        let results = vec![make_func("process", "src/lib.rs", 10)];
        let mut lcov = HashMap::new();
        lcov.insert(
            "src/lib.rs".to_string(),
            make_lcov_data(&[("process", 0)], &[]),
        );
        let warnings = detect_uncovered_functions(&results, &lcov);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, TqWarningKind::Uncovered);
    }

    #[test]
    fn test_covered_function_no_warning() {
        let results = vec![make_func("process", "src/lib.rs", 10)];
        let mut lcov = HashMap::new();
        lcov.insert(
            "src/lib.rs".to_string(),
            make_lcov_data(&[("process", 5)], &[]),
        );
        let warnings = detect_uncovered_functions(&results, &lcov);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_function_not_in_lcov_no_warning() {
        let results = vec![make_func("process", "src/lib.rs", 10)];
        let lcov = HashMap::new();
        let warnings = detect_uncovered_functions(&results, &lcov);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_test_function_excluded() {
        let results = vec![make_func("test_something", "src/lib.rs", 10)];
        let mut lcov = HashMap::new();
        lcov.insert(
            "src/lib.rs".to_string(),
            make_lcov_data(&[("test_something", 0)], &[]),
        );
        let warnings = detect_uncovered_functions(&results, &lcov);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_suppressed_function_excluded() {
        let mut func = make_func("process", "src/lib.rs", 10);
        func.suppressed = true;
        let results = vec![func];
        let mut lcov = HashMap::new();
        lcov.insert(
            "src/lib.rs".to_string(),
            make_lcov_data(&[("process", 0)], &[]),
        );
        let warnings = detect_uncovered_functions(&results, &lcov);
        assert!(warnings.is_empty());
    }

    // ── TQ-005 tests ────────────────────────────────────────

    #[test]
    fn test_untested_logic_detected() {
        let mut func = make_func("process", "src/lib.rs", 10);
        func.complexity = Some(ComplexityMetrics {
            logic_occurrences: vec![LogicOccurrence {
                kind: "if".to_string(),
                line: 15,
            }],
            ..Default::default()
        });
        let results = vec![func];
        let mut lcov = HashMap::new();
        lcov.insert("src/lib.rs".to_string(), make_lcov_data(&[], &[(15, 0)]));
        let warnings = detect_untested_logic(&results, &lcov);
        assert_eq!(warnings.len(), 1);
        match &warnings[0].kind {
            TqWarningKind::UntestedLogic { uncovered_lines } => {
                assert_eq!(uncovered_lines.len(), 1);
                assert_eq!(uncovered_lines[0], ("if".to_string(), 15));
            }
            _ => panic!("expected UntestedLogic"),
        }
    }

    #[test]
    fn test_covered_logic_no_warning() {
        let mut func = make_func("process", "src/lib.rs", 10);
        func.complexity = Some(ComplexityMetrics {
            logic_occurrences: vec![LogicOccurrence {
                kind: "if".to_string(),
                line: 15,
            }],
            ..Default::default()
        });
        let results = vec![func];
        let mut lcov = HashMap::new();
        lcov.insert("src/lib.rs".to_string(), make_lcov_data(&[], &[(15, 3)]));
        let warnings = detect_untested_logic(&results, &lcov);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_logic_line_not_in_lcov_no_warning() {
        let mut func = make_func("process", "src/lib.rs", 10);
        func.complexity = Some(ComplexityMetrics {
            logic_occurrences: vec![LogicOccurrence {
                kind: "if".to_string(),
                line: 15,
            }],
            ..Default::default()
        });
        let results = vec![func];
        let lcov = HashMap::new(); // no LCOV data at all
        let warnings = detect_untested_logic(&results, &lcov);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_no_logic_no_warning() {
        let func = make_func("process", "src/lib.rs", 10);
        let results = vec![func];
        let mut lcov = HashMap::new();
        lcov.insert("src/lib.rs".to_string(), make_lcov_data(&[], &[(15, 0)]));
        let warnings = detect_untested_logic(&results, &lcov);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_multiple_uncovered_logic_lines_one_warning() {
        let mut func = make_func("process", "src/lib.rs", 10);
        func.complexity = Some(ComplexityMetrics {
            logic_occurrences: vec![
                LogicOccurrence {
                    kind: "if".to_string(),
                    line: 15,
                },
                LogicOccurrence {
                    kind: "match".to_string(),
                    line: 20,
                },
            ],
            ..Default::default()
        });
        let results = vec![func];
        let mut lcov = HashMap::new();
        lcov.insert(
            "src/lib.rs".to_string(),
            make_lcov_data(&[], &[(15, 0), (20, 0)]),
        );
        let warnings = detect_untested_logic(&results, &lcov);
        assert_eq!(
            warnings.len(),
            1,
            "one warning per function, not per logic line"
        );
        match &warnings[0].kind {
            TqWarningKind::UntestedLogic { uncovered_lines } => {
                assert_eq!(uncovered_lines.len(), 2);
            }
            _ => panic!("expected UntestedLogic"),
        }
    }
}
