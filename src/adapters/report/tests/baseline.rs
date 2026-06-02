use crate::adapters::analyzers::iosp::{
    CallOccurrence, Classification, FunctionAnalysis, LogicOccurrence,
};
use crate::adapters::report::test_support::{make_result, violation};
use crate::report::baseline::{create_baseline, print_comparison};
use crate::report::Summary;

fn make_summary(results: &[FunctionAnalysis]) -> Summary {
    let mut s = Summary::from_results(results);
    s.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    s
}

/// Build a baseline from `results` and parse it back into a JSON `Value`.
fn baseline_json(results: &[FunctionAnalysis]) -> serde_json::Value {
    let summary = make_summary(results);
    serde_json::from_str(&create_baseline(results, &summary)).unwrap()
}

/// Build a baseline from `old`, then compare `new` against it — returns
/// whether `new` is a regression.
fn compare(old: &[FunctionAnalysis], new: &[FunctionAnalysis]) -> bool {
    let baseline = create_baseline(old, &make_summary(old));
    print_comparison(&baseline, new, &make_summary(new))
}

/// `[a: Integration, b: <b_class>]` — the two-function fixture the
/// comparison tests vary.
fn ab(b_class: Classification) -> Vec<FunctionAnalysis> {
    vec![
        make_result("a", Classification::Integration),
        make_result("b", b_class),
    ]
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
fn test_create_baseline_perfect_scores_are_one() {
    // With no violations both the IOSP and the overall quality score are 1.0.
    let parsed = baseline_json(&ab(Classification::Operation));
    for field in ["iosp_score", "quality_score"] {
        let score = parsed[field].as_f64().unwrap();
        assert!(
            (score - 1.0).abs() < 1e-10,
            "{field} should be 1.0 with no violations; got {score}"
        );
    }
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
fn test_print_comparison_detects_only_regressions() {
    // `print_comparison` flags a regression only when the new quality score
    // is worse than the baseline: identical scores and improvements pass,
    // a new violation (lower score) regresses. (label, old, new, regressed)
    let cases: Vec<(&str, Vec<FunctionAnalysis>, Vec<FunctionAnalysis>, bool)> = vec![
        (
            "identical scores → no regression",
            vec![make_result("a", Classification::Integration)],
            vec![make_result("a", Classification::Integration)],
            false,
        ),
        (
            "improvement (violation removed) → no regression",
            ab(violation("if", "x")),
            ab(Classification::Operation),
            false,
        ),
        (
            "new violation (lower score) → regression",
            ab(Classification::Operation),
            ab(violation("if", "x")),
            true,
        ),
    ];
    for (label, old, new, regressed) in cases {
        assert_eq!(compare(&old, &new), regressed, "case {label}");
    }
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
    // V2 baselines compare by quality_score: a perfect-score baseline that
    // gains a violation must regress. (Covered structurally by
    // `test_print_comparison_detects_only_regressions`; this pins the V2
    // quality-score path explicitly.)
    let regressed = compare(&ab(Classification::Operation), &ab(violation("if", "x")));
    assert!(regressed, "Quality score regression should be detected");
}
