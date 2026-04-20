use std::collections::{HashMap, HashSet};

use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};
use crate::config::Config;
use crate::findings::Suppression;
use crate::report::Summary;

/// Remove self-calls from `own_calls` for functions marked with `// qual:recursive`.
/// Operation: iterates results, checks annotation window, filters self-calls.
pub(crate) fn apply_recursive_annotations(
    results: &mut [FunctionAnalysis],
    recursive_lines: &HashMap<String, HashSet<usize>>,
) {
    results.iter_mut().for_each(|fa| {
        let is_marked = recursive_lines
            .get(&fa.file)
            .map(|lines| crate::findings::has_annotation_in_window(lines, fa.line))
            .unwrap_or(false);
        if is_marked {
            let self_name = &fa.name;
            let qualified = &fa.qualified_name;
            fa.own_calls
                .retain(|call| call != self_name && call != qualified);
        }
    });
}

/// Reclassify Violations whose own calls all target safe functions.
/// Safe = any non-Violation (Operations, Trivials, and Integrations).
/// Iterates until stable to handle cascading reclassification.
/// Operation: loop + set operations, no own calls.
pub(crate) fn apply_leaf_reclassification(results: &mut [FunctionAnalysis]) {
    loop {
        let safe_names: HashSet<String> = results
            .iter()
            .filter(|f| !matches!(f.classification, Classification::Violation { .. }))
            .flat_map(|f| {
                [
                    f.name.clone(),
                    f.qualified_name.clone(),
                    format!(".{}()", f.name),
                ]
            })
            .collect();

        let mut changed = false;
        results.iter_mut().for_each(|fa| {
            if matches!(fa.classification, Classification::Violation { .. })
                && fa.own_calls.iter().all(|call| safe_names.contains(call))
            {
                fa.classification = Classification::Operation;
                fa.own_calls.clear();
                fa.severity = None;
                fa.effort_score = None;
                changed = true;
            }
        });

        if !changed {
            break;
        }
    }
}

/// Reclassify IOSP violations in test functions as Trivial.
/// Operation: iterates results, reclassifies matching entries.
pub(super) fn exclude_test_violations(results: &mut [FunctionAnalysis]) {
    results
        .iter_mut()
        .filter(|fa| fa.is_test && matches!(fa.classification, Classification::Violation { .. }))
        .for_each(|fa| {
            fa.classification = Classification::Trivial;
            fa.severity = None;
            fa.effort_score = None;
        });
}

/// Apply IOSP and complexity suppression flags to a function analysis.
/// Operation: checks suppression lines against function line, sets suppressed flags.
pub(super) fn apply_file_suppressions(fa: &mut FunctionAnalysis, suppressions: &[Suppression]) {
    let covers_iosp = |s: &Suppression| s.covers(crate::findings::Dimension::Iosp);
    let covers_cx = |s: &Suppression| s.covers(crate::findings::Dimension::Complexity);
    let window = crate::findings::ANNOTATION_WINDOW;
    let is_adjacent = |s: &Suppression| s.line <= fa.line && fa.line - s.line <= window;

    fa.suppressed = fa.suppressed
        || suppressions
            .iter()
            .any(|s| is_adjacent(s) && covers_iosp(s));
    fa.complexity_suppressed =
        fa.complexity_suppressed || suppressions.iter().any(|s| is_adjacent(s) && covers_cx(s));
}

/// Set cognitive/cyclomatic complexity and magic number warning flags.
/// Operation: iterates results applying cognitive, cyclomatic, and magic number threshold checks.
pub(super) fn apply_complexity_warnings(
    results: &mut [FunctionAnalysis],
    config: &Config,
    summary: &mut Summary,
) {
    if !config.complexity.enabled {
        return;
    }
    for fa in results.iter_mut() {
        if fa.suppressed || fa.complexity_suppressed {
            continue;
        }
        if let Some(ref m) = fa.complexity {
            if m.cognitive_complexity > config.complexity.max_cognitive
                || m.cyclomatic_complexity > config.complexity.max_cyclomatic
            {
                fa.cognitive_warning = m.cognitive_complexity > config.complexity.max_cognitive;
                fa.cyclomatic_warning = m.cyclomatic_complexity > config.complexity.max_cyclomatic;
                summary.complexity_warnings += 1;
            }
            // Magic numbers are expected in tests (assert_eq!(x, 42) etc.),
            // so skip test functions for this specific check.
            if !fa.is_test {
                summary.magic_number_warnings += m.magic_numbers.len();
            }
        }
    }
}

/// Check if a function has error-handling issues (unwrap/panic/todo/expect).
/// Skips test functions — unwrap() is idiomatic in tests.
/// Operation: arithmetic comparison logic.
fn has_error_handling_issue(
    fa: &FunctionAnalysis,
    m: &crate::adapters::analyzers::iosp::ComplexityMetrics,
    check_errors: bool,
    expect_threshold: usize,
) -> bool {
    check_errors
        && !fa.is_test
        && (m.unwrap_count + m.panic_count + m.todo_count + m.expect_count.min(expect_threshold)
            > 0)
}

/// Check if a function exceeds the length threshold in production code.
/// Tests are excluded — arrange-act-assert sequences are legitimately long.
/// Operation: trivial field read.
fn is_production_length_over(
    fa: &FunctionAnalysis,
    m: &crate::adapters::analyzers::iosp::ComplexityMetrics,
    max_lines: usize,
) -> bool {
    if fa.is_test {
        return false;
    }
    m.function_lines > max_lines
}

/// Check if a function has a `// qual:allow(unsafe)` annotation within the window.
/// Operation: delegation to has_annotation_in_window.
fn is_unsafe_allowed(
    fa: &FunctionAnalysis,
    unsafe_allow_lines: &HashMap<String, HashSet<usize>>,
) -> bool {
    unsafe_allow_lines
        .get(&fa.file)
        .map(|lines| crate::findings::has_annotation_in_window(lines, fa.line))
        .unwrap_or(false)
}

pub(super) fn apply_extended_warnings(
    results: &mut [FunctionAnalysis],
    config: &Config,
    summary: &mut Summary,
    unsafe_allow_lines: &HashMap<String, HashSet<usize>>,
) {
    if !config.complexity.enabled {
        return;
    }
    let max_nesting = config.complexity.max_nesting_depth;
    let max_lines = config.complexity.max_function_lines;
    let check_unsafe = config.complexity.detect_unsafe;
    let check_errors = config.complexity.detect_error_handling;
    let expect_threshold = if config.complexity.allow_expect { 0 } else { 1 };

    let is_active = |fa: &FunctionAnalysis| !fa.suppressed && !fa.complexity_suppressed;

    let has_unsafe_issue =
        |fa: &FunctionAnalysis, m: &crate::adapters::analyzers::iosp::ComplexityMetrics| {
            check_unsafe && m.unsafe_blocks > 0 && !is_unsafe_allowed(fa, unsafe_allow_lines)
        };

    let check_err = |fa: &FunctionAnalysis,
                     m: &crate::adapters::analyzers::iosp::ComplexityMetrics| {
        has_error_handling_issue(fa, m, check_errors, expect_threshold)
    };

    // Tests legitimately contain long arrange-act-assert sequences; skip
    // LONG_FN for them to keep the check focused on production code.
    let has_length_issue =
        |fa: &FunctionAnalysis, m: &crate::adapters::analyzers::iosp::ComplexityMetrics| {
            is_production_length_over(fa, m, max_lines)
        };

    results
        .iter_mut()
        .filter(|fa| is_active(fa))
        .for_each(|fa| {
            let m = match fa.complexity {
                Some(ref m) => m,
                None => return,
            };
            if m.max_nesting > max_nesting {
                fa.nesting_depth_warning = true;
                summary.nesting_depth_warnings += 1;
            }
            if has_length_issue(fa, m) {
                fa.function_length_warning = true;
                summary.function_length_warnings += 1;
            }
            if has_unsafe_issue(fa, m) {
                fa.unsafe_warning = true;
                summary.unsafe_warnings += 1;
            }
            if check_err(fa, m) {
                fa.error_handling_warning = true;
                summary.error_handling_warnings += 1;
            }
        });
}

/// Count `#[allow(` attributes in production code, excluding test module attributes.
/// Operation: line-scanning logic with backward walk for attribute grouping.
pub(crate) fn count_rust_allow_attrs(source: &str) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    let mut cutoff = lines.len();
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == "#[cfg(test)]" {
            cutoff = i;
            while cutoff > 0 && lines[cutoff - 1].trim().starts_with("#[") {
                cutoff -= 1;
            }
            break;
        }
    }
    lines[..cutoff]
        .iter()
        .filter(|line| line.trim().starts_with("#[allow("))
        .count()
}

/// Count all suppression markers: `// qual:allow` comments + `#[allow(...)]` Rust attributes.
/// Operation: scans suppression map and source text for both suppression patterns.
pub(super) fn count_all_suppressions(
    suppression_lines: &std::collections::HashMap<String, Vec<crate::findings::Suppression>>,
    parsed: &[(String, String, syn::File)],
) -> usize {
    let qual_count: usize = suppression_lines.values().map(|v| v.len()).sum();
    let rust_count: usize = parsed
        .iter()
        .map(|(_, source, _)| count_rust_allow_attrs(source))
        .sum();
    qual_count + rust_count
}

/// Check if the suppression ratio exceeds the configured maximum.
/// Operation: arithmetic comparison logic.
pub(super) fn check_suppression_ratio(total: usize, suppressed: usize, max_ratio: f64) -> bool {
    if total == 0 {
        return false;
    }
    (suppressed as f64 / total as f64) > max_ratio
}

// Orphan-suppression detection lives in `super::orphan_suppressions`.
