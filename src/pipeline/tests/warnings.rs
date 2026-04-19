use crate::adapters::analyzers::iosp::{Classification, ComplexityMetrics, FunctionAnalysis};
use crate::config::Config;
use crate::pipeline::warnings::*;
use crate::report::Summary;
use std::collections::{HashMap, HashSet};

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
    let source = "#[allow(clippy::too_many_arguments)]\nfn big() {}\n\n#[cfg(test)]\nmod tests {}";
    assert_eq!(count_rust_allow_attrs(source), 1);
}

#[test]
fn test_allow_inside_test_module_excluded() {
    let source = "fn good() {}\n#[cfg(test)]\nmod tests {\n#[allow(dead_code)]\nfn helper() {}\n}";
    assert_eq!(count_rust_allow_attrs(source), 0);
}

// ── apply_extended_warnings ───────────────────────────────────

use crate::adapters::analyzers::iosp::compute_severity;

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
    fa.severity = Some(crate::adapters::analyzers::iosp::Severity::Low);
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
