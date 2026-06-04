use std::collections::{HashMap, HashSet};

use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};
use crate::app::complexity_suppressions::{bool_warns, metric_warns};
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
    // Only a BLANKET `allow(complexity)` (no target) silences the whole
    // dimension; targeted `allow(complexity, <kind>)` forms are handled
    // per-kind in `apply_complexity_warnings` / `apply_extended_warnings`.
    let blanket_cx =
        |s: &Suppression| s.covers(crate::findings::Dimension::Complexity) && s.target.is_none();
    let window = crate::findings::ANNOTATION_WINDOW;
    let is_adjacent = |s: &Suppression| s.line <= fa.line && fa.line - s.line <= window;

    fa.suppressed = fa.suppressed
        || suppressions
            .iter()
            .any(|s| is_adjacent(s) && covers_iosp(s));
    fa.complexity_suppressed =
        fa.complexity_suppressed || suppressions.iter().any(|s| is_adjacent(s) && blanket_cx(s));
}

/// Set cognitive/cyclomatic complexity and magic number warning flags.
/// Operation: iterates results applying cognitive, cyclomatic, and magic number threshold checks.
pub(super) fn apply_complexity_warnings(
    results: &mut [FunctionAnalysis],
    config: &Config,
    summary: &mut Summary,
    suppression_lines: &HashMap<String, Vec<Suppression>>,
) {
    if !config.complexity.enabled {
        return;
    }
    for fa in results.iter_mut() {
        if fa.suppressed || fa.complexity_suppressed {
            continue;
        }
        // Copy the metrics out so the per-kind suppression check (which borrows
        // `fa`) doesn't overlap the warning-flag writes.
        let Some((cognitive, cyclomatic, magic_count)) = fa.complexity.as_ref().map(|m| {
            (
                m.cognitive_complexity,
                m.cyclomatic_complexity,
                m.magic_numbers.len(),
            )
        }) else {
            continue;
        };
        let cog = cognitive > config.complexity.max_cognitive;
        let cyc = cyclomatic > config.complexity.max_cyclomatic;
        fa.cognitive_warning = metric_warns(
            suppression_lines,
            fa,
            cog,
            cognitive as f64,
            "max_cognitive",
        );
        fa.cyclomatic_warning = metric_warns(
            suppression_lines,
            fa,
            cyc,
            cyclomatic as f64,
            "max_cyclomatic",
        );
        if fa.cognitive_warning || fa.cyclomatic_warning {
            summary.complexity_warnings += 1;
        }
        // Magic numbers are expected in tests (assert_eq!(x, 42) etc.), so
        // skip test functions for this specific check.
        if bool_warns(suppression_lines, fa, !fa.is_test, "magic_numbers") {
            summary.magic_number_warnings += magic_count;
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

/// Check if a function exceeds the LONG_FN length threshold. Test fns use the
/// `[tests].max_function_lines` override (which defaults to the production
/// limit); production fns use `[complexity].max_function_lines`.
/// Operation: threshold selection + comparison.
fn is_length_over(
    fa: &FunctionAnalysis,
    m: &crate::adapters::analyzers::iosp::ComplexityMetrics,
    prod_max: usize,
    test_max: usize,
) -> bool {
    let max = if fa.is_test { test_max } else { prod_max };
    m.function_lines > max
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

/// Config-derived thresholds/toggles for the extended complexity checks.
struct ExtendedCx {
    max_nesting: usize,
    max_lines: usize,
    test_max_lines: usize,
    check_unsafe: bool,
    check_errors: bool,
    expect_threshold: usize,
}

impl ExtendedCx {
    /// Operation: field reads + one ternary, no own calls.
    fn from(config: &Config) -> Self {
        let cx = &config.complexity;
        Self {
            max_nesting: cx.max_nesting_depth,
            max_lines: cx.max_function_lines,
            test_max_lines: config
                .tests
                .max_function_lines
                .unwrap_or(cx.max_function_lines),
            check_unsafe: cx.detect_unsafe,
            check_errors: cx.detect_error_handling,
            expect_threshold: if cx.allow_expect { 0 } else { 1 },
        }
    }
}

/// Integration: filters active fns and applies the extended checks to each.
pub(super) fn apply_extended_warnings(
    results: &mut [FunctionAnalysis],
    config: &Config,
    summary: &mut Summary,
    unsafe_allow_lines: &HashMap<String, HashSet<usize>>,
    suppression_lines: &HashMap<String, Vec<Suppression>>,
) {
    if !config.complexity.enabled {
        return;
    }
    let cx = ExtendedCx::from(config);
    results
        .iter_mut()
        .filter(|fa| !fa.suppressed && !fa.complexity_suppressed)
        .for_each(|fa| {
            apply_extended_to_fn(fa, &cx, unsafe_allow_lines, suppression_lines, summary)
        });
}

/// Apply nesting / length / unsafe / error-handling checks to one function,
/// each honouring a targeted `allow(complexity, <kind>)`. LONG_FN applies to
/// test fns too (at `[tests].max_function_lines`, defaulting to production).
/// Operation: per-kind decisions reaching non-violation helpers.
fn apply_extended_to_fn(
    fa: &mut FunctionAnalysis,
    cx: &ExtendedCx,
    unsafe_allow_lines: &HashMap<String, HashSet<usize>>,
    suppression_lines: &HashMap<String, Vec<Suppression>>,
    summary: &mut Summary,
) {
    let Some(m) = fa.complexity.clone() else {
        return;
    };
    let nesting_over = metric_warns(
        suppression_lines,
        fa,
        m.max_nesting > cx.max_nesting,
        m.max_nesting as f64,
        "max_nesting_depth",
    );
    let length_over = metric_warns(
        suppression_lines,
        fa,
        is_length_over(fa, &m, cx.max_lines, cx.test_max_lines),
        m.function_lines as f64,
        "max_function_lines",
    );
    let unsafe_present =
        cx.check_unsafe && m.unsafe_blocks > 0 && !is_unsafe_allowed(fa, unsafe_allow_lines);
    let unsafe_over = bool_warns(suppression_lines, fa, unsafe_present, "unsafe");
    let err_present = has_error_handling_issue(fa, &m, cx.check_errors, cx.expect_threshold);
    let err_over = bool_warns(suppression_lines, fa, err_present, "error_handling");
    flag_and_count(
        nesting_over,
        &mut fa.nesting_depth_warning,
        &mut summary.nesting_depth_warnings,
    );
    flag_and_count(
        length_over,
        &mut fa.function_length_warning,
        &mut summary.function_length_warnings,
    );
    flag_and_count(
        unsafe_over,
        &mut fa.unsafe_warning,
        &mut summary.unsafe_warnings,
    );
    flag_and_count(
        err_over,
        &mut fa.error_handling_warning,
        &mut summary.error_handling_warnings,
    );
}

/// Set a warning flag and bump its counter when `over` is true.
/// Operation: single conditional.
fn flag_and_count(over: bool, flag: &mut bool, count: &mut usize) {
    if over {
        *flag = true;
        *count += 1;
    }
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
