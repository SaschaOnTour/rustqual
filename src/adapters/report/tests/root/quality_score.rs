use super::*;

#[test]
fn test_quality_score_perfect() {
    let results = vec![
        make_result("a", Classification::Integration),
        make_result("b", Classification::Operation),
    ];
    let mut summary = Summary::from_results(&results);
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!((summary.quality_score - 1.0).abs() < 1e-10);
}

#[test]
fn test_quality_score_with_violations() {
    let results = vec![
        make_result("a", Classification::Integration),
        make_result(
            "b",
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
        ),
    ];
    let mut summary = Summary::from_results(&results);
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!(summary.quality_score < 1.0);
    assert!(summary.quality_score > 0.0);
}

#[test]
fn test_quality_score_empty() {
    let results: Vec<FunctionAnalysis> = vec![];
    let mut summary = Summary::from_results(&results);
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!((summary.quality_score - 1.0).abs() < 1e-10);
}

#[test]
fn test_quality_score_with_warnings() {
    let results = vec![
        make_result("a", Classification::Integration),
        make_result("b", Classification::Operation),
        make_result("c", Classification::Operation),
        make_result("d", Classification::Operation),
    ];
    let mut summary = Summary::from_results(&results);
    summary.complexity_warnings = 2;
    summary.duplicate_groups = 1;
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!(summary.quality_score < 1.0);
    assert!(summary.dimension_scores[1] < 1.0); // complexity
    assert!(summary.dimension_scores[2] < 1.0); // DRY
}

#[test]
fn test_score_reflects_total_findings_realistically() {
    // 100 functions, 10 IOSP violations + 10 complexity warnings = 20 findings
    // With default weights (IOSP=0.25, CX=0.20), score should be significantly < 90%
    let mut summary = Summary {
        total: 100,
        violations: 10,
        iosp_score: 0.9, // 10/100 violations
        complexity_warnings: 10,
        ..Default::default()
    };
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!(
        summary.quality_score < 0.85,
        "20 findings / 100 functions should be < 85%, got {:.1}%",
        summary.quality_score * 100.0
    );
    assert!(
        summary.quality_score > 0.50,
        "20 findings / 100 functions should be > 50%, got {:.1}%",
        summary.quality_score * 100.0
    );
}

#[test]
fn test_score_100_percent_only_with_zero_findings() {
    // Any finding should prevent 100%
    let mut summary = Summary {
        total: 100,
        iosp_score: 1.0,
        magic_number_warnings: 1,
        ..Default::default()
    };
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!(
        summary.quality_score < 1.0,
        "1 finding should prevent 100%, got {:.1}%",
        summary.quality_score * 100.0
    );
}

#[test]
fn test_score_all_violations_is_near_zero() {
    // 100/100 IOSP violations → score should be very low, not 75%
    let mut summary = Summary {
        total: 100,
        violations: 100,
        iosp_score: 0.0, // 100% violations
        ..Default::default()
    };
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!(
        summary.quality_score < 0.10,
        "100% violations should give score < 10%, got {:.1}%",
        summary.quality_score * 100.0
    );
}

#[test]
fn test_total_findings() {
    let summary = Summary {
        violations: 1,
        complexity_warnings: 2,
        magic_number_warnings: 1,
        duplicate_groups: 1,
        coupling_cycles: 1,
        ..Summary::default()
    };
    assert_eq!(summary.total_findings(), 6);
}

#[test]
fn test_complexity_in_function_analysis() {
    let func = FunctionAnalysis {
        name: "f".to_string(),
        file: "test.rs".to_string(),
        line: 1,
        classification: Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: Some(ComplexityMetrics {
            logic_count: 3,
            call_count: 0,
            max_nesting: 2,
            ..Default::default()
        }),
        qualified_name: "f".to_string(),
        severity: None,
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
    };
    assert_eq!(func.complexity.as_ref().unwrap().logic_count, 3);
    assert_eq!(func.complexity.as_ref().unwrap().max_nesting, 2);
}

#[test]
fn test_suppression_ratio_default_false() {
    let summary = Summary::default();
    assert!(!summary.suppression_ratio_exceeded);
}

#[test]
fn test_suppression_ratio_flag_preserved() {
    let summary = Summary {
        suppression_ratio_exceeded: true,
        ..Summary::default()
    };
    assert!(summary.suppression_ratio_exceeded);
}
