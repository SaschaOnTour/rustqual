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
fn compute_quality_score_pins_every_dimension_deficit() {
    // Each dimension's deficit is `1 - (sum_of_its_findings / total).min(1)`.
    // With total = 100 and one finding in every contributing field, each
    // dimension score is an exact, distinct value. Asserting all seven pins
    // every `+` in the per-dimension sums, the `coupling_cycles * 2` weight, the
    // `/ total` divisions, and the leading `1.0 -` — any of those mutations
    // shifts the corresponding score off its expected value.
    let mut summary = Summary {
        total: 100,
        iosp_score: 0.80,
        // complexity: 6 fields → 6
        complexity_warnings: 1,
        magic_number_warnings: 1,
        nesting_depth_warnings: 1,
        function_length_warnings: 1,
        unsafe_warnings: 1,
        error_handling_warnings: 1,
        // dry: 6 fields → 6
        duplicate_groups: 1,
        fragment_groups: 1,
        dead_code_warnings: 1,
        boilerplate_warnings: 1,
        wildcard_import_warnings: 1,
        repeated_match_groups: 1,
        // srp: 4 fields → 4
        srp_struct_warnings: 1,
        srp_module_warnings: 1,
        srp_param_warnings: 1,
        structural_srp_warnings: 1,
        // coupling: 1 + 1*2 + 1 + 1 = 5
        coupling_warnings: 1,
        coupling_cycles: 1,
        sdp_violations: 1,
        structural_coupling_warnings: 1,
        // tq: 5 fields → 5
        tq_no_assertion_warnings: 1,
        tq_no_sut_warnings: 1,
        tq_untested_warnings: 1,
        tq_uncovered_warnings: 1,
        tq_untested_logic_warnings: 1,
        // architecture → 1
        architecture_warnings: 1,
        ..Default::default()
    };
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    let expected = [0.80, 0.94, 0.94, 0.96, 0.95, 0.95, 0.99];
    for (i, want) in expected.iter().enumerate() {
        assert!(
            (summary.dimension_scores[i] - want).abs() < 1e-9,
            "dimension_scores[{i}] = {} want {want}",
            summary.dimension_scores[i]
        );
    }
}

#[test]
fn compute_quality_score_zero_weights_collapse_to_zero() {
    // With every weight zero, `active_dims` is 0, so the `active_dims > 0.0`
    // guard falls to `scale = 1.0` and the score is `1 - 1*(1 - 0) = 0`.
    // Flipping that guard to `>=` would take `scale = active_dims = 0.0`,
    // yielding 1.0 — caught by asserting exactly 0.0.
    let mut summary = Summary {
        total: 100,
        iosp_score: 0.9,
        ..Default::default()
    };
    summary.compute_quality_score(&[0.0; 7]);
    assert!(
        summary.quality_score.abs() < 1e-12,
        "all-zero weights → score 0, got {}",
        summary.quality_score
    );
}

#[test]
fn compute_quality_score_epsilon_weight_is_inactive() {
    // A weight of exactly `f64::EPSILON` is *not* an active dimension
    // (`w > f64::EPSILON` is false). With two real weights of 0.5 the scale is
    // 2; counting the epsilon weight as active (the `>`→`>=` mutation) would
    // make it 3 and pull the score from 0.90 to 0.85.
    let mut summary = Summary {
        total: 100,
        iosp_score: 0.9,
        ..Default::default()
    };
    let mut weights = [0.0; 7];
    weights[0] = 0.5;
    weights[1] = 0.5;
    weights[2] = f64::EPSILON;
    summary.compute_quality_score(&weights);
    // dimension_scores = [0.9, 1, 1, ...]; weighted_avg = 0.9*0.5 + 1*0.5 ≈ 0.95;
    // scale = 2 → 1 - 2*(1 - 0.95) = 0.90.
    assert!(
        (summary.quality_score - 0.90).abs() < 1e-9,
        "epsilon weight must not count as active (scale 2 → 0.90), got {}",
        summary.quality_score
    );
}

#[test]
fn total_findings_sums_every_contributing_field() {
    // Every one of the 28 finding-count fields set to 1 → total 28. With all
    // fields non-zero, any `+`→`-`/`*` in the sum changes the result, so the
    // exact assertion pins the whole `total_findings` chain.
    let summary = Summary {
        violations: 1,
        complexity_warnings: 1,
        magic_number_warnings: 1,
        nesting_depth_warnings: 1,
        function_length_warnings: 1,
        unsafe_warnings: 1,
        error_handling_warnings: 1,
        duplicate_groups: 1,
        fragment_groups: 1,
        dead_code_warnings: 1,
        boilerplate_warnings: 1,
        srp_struct_warnings: 1,
        srp_module_warnings: 1,
        srp_param_warnings: 1,
        wildcard_import_warnings: 1,
        repeated_match_groups: 1,
        coupling_warnings: 1,
        coupling_cycles: 1,
        sdp_violations: 1,
        tq_no_assertion_warnings: 1,
        tq_no_sut_warnings: 1,
        tq_untested_warnings: 1,
        tq_uncovered_warnings: 1,
        tq_untested_logic_warnings: 1,
        structural_srp_warnings: 1,
        structural_coupling_warnings: 1,
        architecture_warnings: 1,
        orphan_suppressions: 1,
        ..Summary::default()
    };
    assert_eq!(summary.total_findings(), 28);
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
