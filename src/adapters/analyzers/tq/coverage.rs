use std::collections::HashMap;

use crate::adapters::analyzers::iosp::FunctionAnalysis;

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
