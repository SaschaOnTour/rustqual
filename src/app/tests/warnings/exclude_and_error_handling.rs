use super::*;

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
