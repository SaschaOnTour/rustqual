use crate::adapters::analyzers::iosp::{
    compute_severity, CallOccurrence, Classification, FunctionAnalysis, LogicOccurrence,
};
use crate::report::baseline::{create_baseline, print_comparison};
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

fn make_summary(results: &[FunctionAnalysis]) -> Summary {
    let mut s = Summary::from_results(results);
    s.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    s
}

#[test]
fn test_create_baseline_empty() {
    let results: Vec<FunctionAnalysis> = vec![];
    let summary = make_summary(&results);
    let json = create_baseline(&results, &summary);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["violations"].as_u64().unwrap(), 0);
    assert!(parsed["violation_details"].as_array().unwrap().is_empty());
}

#[test]
fn test_create_baseline_with_violations() {
    let results = vec![
        make_result("good_fn", Classification::Integration),
        make_result(
            "bad_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "if".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "helper".into(),
                    line: 2,
                }],
            },
        ),
    ];
    let summary = make_summary(&results);
    let json = create_baseline(&results, &summary);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["violations"].as_u64().unwrap(), 1);
    assert_eq!(parsed["violation_details"].as_array().unwrap().len(), 1);
}

#[test]
fn test_create_baseline_iosp_score() {
    let results = vec![
        make_result("a", Classification::Integration),
        make_result("b", Classification::Operation),
    ];
    let summary = make_summary(&results);
    let json = create_baseline(&results, &summary);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let score = parsed["iosp_score"].as_f64().unwrap();
    assert!(
        (score - 1.0).abs() < f64::EPSILON,
        "Score should be 1.0 with no violations"
    );
}

#[test]
fn test_create_baseline_is_valid_json() {
    let results = vec![make_result("f", Classification::Operation)];
    let summary = make_summary(&results);
    let json = create_baseline(&results, &summary);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json);
    assert!(parsed.is_ok(), "Baseline must be valid JSON");
}

#[test]
fn test_create_baseline_suppressed_excluded() {
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
    let summary = make_summary(&results);
    let json = create_baseline(&results, &summary);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed["violation_details"].as_array().unwrap().is_empty(),
        "Suppressed violations should not appear in baseline"
    );
}

#[test]
fn test_print_comparison_no_regression() {
    let results = vec![make_result("a", Classification::Integration)];
    let summary = make_summary(&results);
    let baseline = create_baseline(&results, &summary);
    let regressed = print_comparison(&baseline, &results, &summary);
    assert!(!regressed, "Same scores should not be a regression");
}

#[test]
fn test_print_comparison_improvement() {
    let old_results = vec![
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
                    name: "x".into(),
                    line: 2,
                }],
            },
        ),
    ];
    let old_summary = make_summary(&old_results);
    let baseline = create_baseline(&old_results, &old_summary);

    let new_results = vec![
        make_result("a", Classification::Integration),
        make_result("b", Classification::Operation),
    ];
    let new_summary = make_summary(&new_results);
    let regressed = print_comparison(&baseline, &new_results, &new_summary);
    assert!(!regressed, "Improvement should not be a regression");
}

#[test]
fn test_print_comparison_regression() {
    let old_results = vec![
        make_result("a", Classification::Integration),
        make_result("b", Classification::Operation),
    ];
    let old_summary = make_summary(&old_results);
    let baseline = create_baseline(&old_results, &old_summary);

    let new_results = vec![
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
                    name: "x".into(),
                    line: 2,
                }],
            },
        ),
    ];
    let new_summary = make_summary(&new_results);
    let regressed = print_comparison(&baseline, &new_results, &new_summary);
    assert!(regressed, "Score regression should be detected");
}

#[test]
fn test_print_comparison_invalid_json() {
    let results = vec![make_result("a", Classification::Integration)];
    let summary = make_summary(&results);
    let regressed = print_comparison("not valid json {{{", &results, &summary);
    assert!(!regressed, "Invalid JSON should return false");
}

// ── v2-specific tests ──────────────────────────────────────────

#[test]
fn test_create_baseline_v2_has_version() {
    let results = vec![make_result("f", Classification::Operation)];
    let summary = make_summary(&results);
    let json = create_baseline(&results, &summary);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["version"].as_u64().unwrap(), 2);
}

#[test]
fn test_create_baseline_v2_has_quality_score() {
    let results = vec![
        make_result("a", Classification::Integration),
        make_result("b", Classification::Operation),
    ];
    let summary = make_summary(&results);
    let json = create_baseline(&results, &summary);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let q = parsed["quality_score"].as_f64().unwrap();
    assert!(
        (q - 1.0).abs() < 1e-10,
        "quality_score near 1.0 (float tolerance): got {q}"
    );
}

#[test]
fn test_create_baseline_v2_has_all_dimensions() {
    let results = vec![make_result("f", Classification::Operation)];
    let mut summary = make_summary(&results);
    summary.complexity_warnings = 1;
    summary.duplicate_groups = 2;
    summary.srp_struct_warnings = 1;
    summary.coupling_warnings = 1;
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    let json = create_baseline(&results, &summary);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["complexity_warnings"].as_u64().unwrap(), 1);
    assert_eq!(parsed["duplicate_groups"].as_u64().unwrap(), 2);
    assert_eq!(parsed["srp_struct_warnings"].as_u64().unwrap(), 1);
    assert_eq!(parsed["coupling_warnings"].as_u64().unwrap(), 1);
    assert_eq!(parsed["total_findings"].as_u64().unwrap(), 5);
}

#[test]
fn test_baseline_v1_compat_no_version() {
    // Simulate a v1 baseline (no version field)
    let v1_json = r#"{"iosp_score":1.0,"violations":0,"total":2,"violation_details":[]}"#;
    let results = vec![make_result("a", Classification::Integration)];
    let summary = make_summary(&results);
    let regressed = print_comparison(v1_json, &results, &summary);
    assert!(!regressed, "V1 baseline with same score should not regress");
}

#[test]
fn test_baseline_v2_regression_by_quality_score() {
    // V2 baseline with perfect score
    let results_old = vec![
        make_result("a", Classification::Integration),
        make_result("b", Classification::Operation),
    ];
    let summary_old = make_summary(&results_old);
    let baseline = create_baseline(&results_old, &summary_old);

    // New results: one violation → lower quality score
    let results_new = vec![
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
                    name: "x".into(),
                    line: 2,
                }],
            },
        ),
    ];
    let summary_new = make_summary(&results_new);
    let regressed = print_comparison(&baseline, &results_new, &summary_new);
    assert!(regressed, "Quality score regression should be detected");
}
