use std::collections::{HashMap, HashSet};

use crate::analyzer::{Classification, FunctionAnalysis};
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
            if !m.magic_numbers.is_empty() {
                summary.magic_number_warnings += 1;
            }
        }
    }
}

/// Check if a function has error-handling issues (unwrap/panic/todo/expect).
/// Skips test functions — unwrap() is idiomatic in tests.
/// Operation: arithmetic comparison logic.
fn has_error_handling_issue(
    fa: &FunctionAnalysis,
    m: &crate::analyzer::ComplexityMetrics,
    check_errors: bool,
    expect_threshold: usize,
) -> bool {
    check_errors
        && !fa.is_test
        && (m.unwrap_count + m.panic_count + m.todo_count + m.expect_count.min(expect_threshold)
            > 0)
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

    let has_unsafe_issue = |fa: &FunctionAnalysis, m: &crate::analyzer::ComplexityMetrics| {
        check_unsafe && m.unsafe_blocks > 0 && !is_unsafe_allowed(fa, unsafe_allow_lines)
    };

    let check_err = |fa: &FunctionAnalysis, m: &crate::analyzer::ComplexityMetrics| {
        has_error_handling_issue(fa, m, check_errors, expect_threshold)
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
            if m.function_lines > max_lines {
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
fn count_rust_allow_attrs(source: &str) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── count_rust_allow_attrs ────────────────────────────────────

    #[test]
    fn test_allow_before_cfg_test_excluded() {
        // #[allow(...)] directly before #[cfg(test)] belongs to the test module
        let source = "#[allow(dead_code)]\n#[cfg(test)]\nmod tests {}";
        assert_eq!(count_rust_allow_attrs(source), 0);
    }

    #[test]
    fn test_allow_with_gap_before_cfg_test_counted() {
        // #[allow(...)] with a non-attribute line gap → production code
        let source = "#[allow(dead_code)]\nfn foo() {}\n#[cfg(test)]\nmod tests {}";
        assert_eq!(count_rust_allow_attrs(source), 1);
    }

    #[test]
    fn test_derive_and_allow_before_cfg_test_excluded() {
        // #[derive(Debug)] + #[allow(...)] both part of test module attribute group
        let source = "#[derive(Debug)]\n#[allow(dead_code)]\n#[cfg(test)]\nmod tests {}";
        assert_eq!(count_rust_allow_attrs(source), 0);
    }

    #[test]
    fn test_no_cfg_test_counts_all() {
        let source = "#[allow(dead_code)]\nfn foo() {}\n#[allow(unused)]\nfn bar() {}";
        assert_eq!(count_rust_allow_attrs(source), 2);
    }

    #[test]
    fn test_cfg_test_on_first_line() {
        let source = "#[cfg(test)]\nmod tests {\n#[allow(dead_code)]\nfn x() {}\n}";
        assert_eq!(count_rust_allow_attrs(source), 0);
    }

    #[test]
    fn test_empty_source() {
        assert_eq!(count_rust_allow_attrs(""), 0);
    }

    #[test]
    fn test_production_allow_before_test_section() {
        let source =
            "#[allow(clippy::too_many_arguments)]\nfn big() {}\n\n#[cfg(test)]\nmod tests {}";
        assert_eq!(count_rust_allow_attrs(source), 1);
    }

    #[test]
    fn test_allow_inside_test_module_excluded() {
        let source =
            "fn good() {}\n#[cfg(test)]\nmod tests {\n#[allow(dead_code)]\nfn helper() {}\n}";
        assert_eq!(count_rust_allow_attrs(source), 0);
    }

    // ── apply_extended_warnings ───────────────────────────────────

    use crate::analyzer::{compute_severity, Classification, ComplexityMetrics, FunctionAnalysis};
    use crate::report::Summary;

    fn make_func_with_metrics(metrics: ComplexityMetrics) -> FunctionAnalysis {
        let severity = compute_severity(&Classification::Operation);
        FunctionAnalysis {
            name: "test_fn".to_string(),
            file: "test.rs".to_string(),
            line: 1,
            classification: Classification::Operation,
            parent_type: None,
            suppressed: false,
            complexity: Some(metrics),
            qualified_name: "test_fn".to_string(),
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
            effort_score: None,
            is_test: false,
        }
    }

    #[test]
    fn test_nesting_depth_warning_set() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut results = vec![make_func_with_metrics(ComplexityMetrics {
            max_nesting: 5,
            ..Default::default()
        })];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(results[0].nesting_depth_warning, "Should flag nesting > 4");
        assert_eq!(summary.nesting_depth_warnings, 1);
    }

    #[test]
    fn test_nesting_depth_at_threshold_no_warning() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut results = vec![make_func_with_metrics(ComplexityMetrics {
            max_nesting: 4,
            ..Default::default()
        })];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(!results[0].nesting_depth_warning, "4 == threshold, no warn");
    }

    #[test]
    fn test_function_length_warning_set() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut results = vec![make_func_with_metrics(ComplexityMetrics {
            function_lines: 61,
            ..Default::default()
        })];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(results[0].function_length_warning, "Should flag >60 lines");
        assert_eq!(summary.function_length_warnings, 1);
    }

    #[test]
    fn test_function_length_at_threshold_no_warning() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut results = vec![make_func_with_metrics(ComplexityMetrics {
            function_lines: 60,
            ..Default::default()
        })];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(
            !results[0].function_length_warning,
            "60 == threshold, no warn"
        );
    }

    #[test]
    fn test_unsafe_warning_set() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut results = vec![make_func_with_metrics(ComplexityMetrics {
            unsafe_blocks: 1,
            ..Default::default()
        })];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(results[0].unsafe_warning, "Should flag unsafe blocks");
        assert_eq!(summary.unsafe_warnings, 1);
    }

    #[test]
    fn test_error_handling_unwrap_warning() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut results = vec![make_func_with_metrics(ComplexityMetrics {
            unwrap_count: 1,
            ..Default::default()
        })];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(results[0].error_handling_warning, "Should flag unwrap");
        assert_eq!(summary.error_handling_warnings, 1);
    }

    #[test]
    fn test_error_handling_expect_allowed() {
        let mut config = Config::default();
        config.complexity.allow_expect = true;
        let mut summary = Summary::default();
        let mut results = vec![make_func_with_metrics(ComplexityMetrics {
            expect_count: 3,
            ..Default::default()
        })];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(
            !results[0].error_handling_warning,
            "expect allowed, no warn"
        );
    }

    #[test]
    fn test_error_handling_expect_not_allowed() {
        let mut config = Config::default();
        config.complexity.allow_expect = false;
        let mut summary = Summary::default();
        let mut results = vec![make_func_with_metrics(ComplexityMetrics {
            expect_count: 1,
            ..Default::default()
        })];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(
            results[0].error_handling_warning,
            "expect not allowed, should warn"
        );
    }

    #[test]
    fn test_suppressed_functions_skipped() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut func = make_func_with_metrics(ComplexityMetrics {
            max_nesting: 10,
            function_lines: 100,
            unsafe_blocks: 3,
            unwrap_count: 5,
            ..Default::default()
        });
        func.suppressed = true;
        let mut results = vec![func];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(!results[0].nesting_depth_warning);
        assert!(!results[0].function_length_warning);
        assert!(!results[0].unsafe_warning);
        assert!(!results[0].error_handling_warning);
    }

    #[test]
    fn test_complexity_suppressed_functions_skipped() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut func = make_func_with_metrics(ComplexityMetrics {
            max_nesting: 10,
            function_lines: 100,
            ..Default::default()
        });
        func.complexity_suppressed = true;
        let mut results = vec![func];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(!results[0].nesting_depth_warning);
        assert!(!results[0].function_length_warning);
    }

    // ── exclude_test_violations ──────────────────────────────────

    #[test]
    fn test_exclude_test_violations_reclassifies() {
        let mut fa = make_func_with_metrics(ComplexityMetrics::default());
        fa.is_test = true;
        fa.classification = Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![],
            call_locations: vec![],
        };
        fa.severity = Some(crate::analyzer::Severity::Low);
        fa.effort_score = Some(3.0);
        let mut results = vec![fa];
        exclude_test_violations(&mut results);
        assert_eq!(results[0].classification, Classification::Trivial);
        assert!(results[0].severity.is_none());
        assert!(results[0].effort_score.is_none());
    }

    #[test]
    fn test_exclude_test_violations_keeps_non_test() {
        let mut fa = make_func_with_metrics(ComplexityMetrics::default());
        fa.is_test = false;
        fa.classification = Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![],
            call_locations: vec![],
        };
        let mut results = vec![fa];
        exclude_test_violations(&mut results);
        assert!(matches!(
            results[0].classification,
            Classification::Violation { .. }
        ));
    }

    // ── error handling skipped for tests ─────────────────────────

    #[test]
    fn test_error_handling_skipped_for_test_fn() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut fa = make_func_with_metrics(ComplexityMetrics {
            unwrap_count: 3,
            ..Default::default()
        });
        fa.is_test = true;
        let mut results = vec![fa];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(!results[0].error_handling_warning);
        assert_eq!(summary.error_handling_warnings, 0);
    }

    #[test]
    fn test_error_handling_flagged_for_non_test_fn() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut fa = make_func_with_metrics(ComplexityMetrics {
            unwrap_count: 1,
            ..Default::default()
        });
        fa.is_test = false;
        let mut results = vec![fa];
        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(results[0].error_handling_warning);
        assert_eq!(summary.error_handling_warnings, 1);
    }

    #[test]
    fn test_unsafe_suppressed_by_allow_annotation() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut fa = make_func_with_metrics(ComplexityMetrics {
            unsafe_blocks: 1,
            ..Default::default()
        });
        fa.line = 5;
        let mut results = vec![fa];

        // qual:allow(unsafe) on line 4 (one line before fn at line 5)
        let unsafe_lines: HashMap<String, HashSet<usize>> =
            [("test.rs".to_string(), [4].into_iter().collect())].into();

        apply_extended_warnings(&mut results, &config, &mut summary, &unsafe_lines);
        assert!(
            !results[0].unsafe_warning,
            "qual:allow(unsafe) should suppress unsafe warning"
        );
        assert_eq!(summary.unsafe_warnings, 0);
    }

    #[test]
    fn test_unsafe_without_allow_still_warned() {
        let config = Config::default();
        let mut summary = Summary::default();
        let mut results = vec![make_func_with_metrics(ComplexityMetrics {
            unsafe_blocks: 1,
            ..Default::default()
        })];

        apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
        assert!(
            results[0].unsafe_warning,
            "Without annotation, unsafe should still warn"
        );
        assert_eq!(summary.unsafe_warnings, 1);
    }
}
