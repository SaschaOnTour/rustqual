use colored::Colorize;

use crate::analyzer::{Classification, FunctionAnalysis, PERCENTAGE_MULTIPLIER};

use super::Summary;

#[derive(serde::Serialize, serde::Deserialize)]
struct Baseline {
    version: u32,
    quality_score: f64,
    iosp_score: f64,
    violations: usize,
    total: usize,
    complexity_warnings: usize,
    magic_number_warnings: usize,
    nesting_depth_warnings: usize,
    function_length_warnings: usize,
    unsafe_warnings: usize,
    error_handling_warnings: usize,
    duplicate_groups: usize,
    dead_code_warnings: usize,
    fragment_groups: usize,
    boilerplate_warnings: usize,
    srp_struct_warnings: usize,
    srp_module_warnings: usize,
    wildcard_import_warnings: usize,
    sdp_violations: usize,
    coupling_warnings: usize,
    coupling_cycles: usize,
    #[serde(default)]
    tq_no_assertion_warnings: usize,
    #[serde(default)]
    tq_no_sut_warnings: usize,
    #[serde(default)]
    tq_untested_warnings: usize,
    #[serde(default)]
    tq_uncovered_warnings: usize,
    #[serde(default)]
    tq_untested_logic_warnings: usize,
    #[serde(default)]
    structural_srp_warnings: usize,
    #[serde(default)]
    structural_coupling_warnings: usize,
    total_findings: usize,
    violation_details: Vec<BaselineViolation>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BaselineViolation {
    name: String,
    file: String,
    line: usize,
}

/// Create a v2 JSON baseline string from analysis results.
/// Operation: serialization logic.
pub fn create_baseline(results: &[FunctionAnalysis], summary: &Summary) -> String {
    let violation_details: Vec<BaselineViolation> = results
        .iter()
        .filter(|f| !f.suppressed && matches!(f.classification, Classification::Violation { .. }))
        .map(|f| BaselineViolation {
            name: f.qualified_name.clone(),
            file: f.file.clone(),
            line: f.line,
        })
        .collect();

    let baseline = Baseline {
        version: 2,
        quality_score: summary.quality_score,
        iosp_score: summary.iosp_score,
        violations: summary.violations,
        total: summary.total,
        complexity_warnings: summary.complexity_warnings,
        magic_number_warnings: summary.magic_number_warnings,
        nesting_depth_warnings: summary.nesting_depth_warnings,
        function_length_warnings: summary.function_length_warnings,
        unsafe_warnings: summary.unsafe_warnings,
        error_handling_warnings: summary.error_handling_warnings,
        duplicate_groups: summary.duplicate_groups,
        dead_code_warnings: summary.dead_code_warnings,
        fragment_groups: summary.fragment_groups,
        boilerplate_warnings: summary.boilerplate_warnings,
        srp_struct_warnings: summary.srp_struct_warnings,
        srp_module_warnings: summary.srp_module_warnings,
        wildcard_import_warnings: summary.wildcard_import_warnings,
        sdp_violations: summary.sdp_violations,
        coupling_warnings: summary.coupling_warnings,
        coupling_cycles: summary.coupling_cycles,
        tq_no_assertion_warnings: summary.tq_no_assertion_warnings,
        tq_no_sut_warnings: summary.tq_no_sut_warnings,
        tq_untested_warnings: summary.tq_untested_warnings,
        tq_uncovered_warnings: summary.tq_uncovered_warnings,
        tq_untested_logic_warnings: summary.tq_untested_logic_warnings,
        structural_srp_warnings: summary.structural_srp_warnings,
        structural_coupling_warnings: summary.structural_coupling_warnings,
        total_findings: summary.total_findings(),
        violation_details,
    };

    serde_json::to_string_pretty(&baseline).expect("Baseline serialization failed")
}

/// Compare current results against a baseline and print delta.
/// Returns true if there was a regression (quality score decreased).
/// Supports v1 (IOSP-only) and v2 (full quality score) baseline formats.
/// Operation: comparison and display logic. Own calls hidden in closures.
pub fn print_comparison(
    baseline_content: &str,
    _results: &[FunctionAnalysis],
    summary: &Summary,
) -> bool {
    let show_delta = |label: &str, old_pct: f64, new_pct: f64| {
        print_score_delta(label, old_pct, new_pct);
    };
    let findings = |s: &Summary| s.total_findings();
    let raw: serde_json::Value = match serde_json::from_str(baseline_content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing baseline: {e}");
            return false;
        }
    };
    let is_v2 = raw.get("version").and_then(|v| v.as_u64()).unwrap_or(0) >= 2;
    println!(
        "\n{}",
        "\u{2550}\u{2550}\u{2550} Baseline Comparison \u{2550}\u{2550}\u{2550}".bold()
    );
    let old_iosp = raw["iosp_score"].as_f64().unwrap_or(0.0);
    let old_violations = raw["violations"].as_u64().unwrap_or(0) as usize;
    let violation_delta = summary.violations as i64 - old_violations as i64;
    println!(
        "  Violations: {} \u{2192} {} ({:+})",
        old_violations, summary.violations, violation_delta
    );
    if is_v2 {
        let old_findings = raw["total_findings"].as_u64().unwrap_or(0) as usize;
        let finding_delta = findings(summary) as i64 - old_findings as i64;
        println!(
            "  Findings:   {} \u{2192} {} ({:+})",
            old_findings, findings(summary), finding_delta
        );
    }
    show_delta(
        "IOSP Score",
        old_iosp * PERCENTAGE_MULTIPLIER,
        summary.iosp_score * PERCENTAGE_MULTIPLIER,
    );
    if is_v2 {
        let tq_keys = ["tq_no_assertion_warnings", "tq_no_sut_warnings", "tq_untested_warnings", "tq_uncovered_warnings", "tq_untested_logic_warnings"];
        let old_tq: u64 = tq_keys.iter().map(|k| raw[*k].as_u64().unwrap_or(0)).sum();
        let new_tq = summary.tq_no_assertion_warnings + summary.tq_no_sut_warnings + summary.tq_untested_warnings + summary.tq_uncovered_warnings + summary.tq_untested_logic_warnings;
        println!("  TQ warnings: {} \u{2192} {} ({:+})", old_tq, new_tq, new_tq as i64 - old_tq as i64);
        let old_quality = raw["quality_score"].as_f64().unwrap_or(0.0);
        show_delta(
            "Quality",
            old_quality * PERCENTAGE_MULTIPLIER,
            summary.quality_score * PERCENTAGE_MULTIPLIER,
        );
        summary.quality_score - old_quality < 0.0
    } else {
        summary.iosp_score - old_iosp < 0.0
    }
}

/// Print a labeled score delta line with colored arrow.
/// Operation: conditional formatting logic, no own calls.
fn print_score_delta(label: &str, old_pct: f64, new_pct: f64) {
    let delta = new_pct - old_pct;
    if delta > 0.0 {
        println!(
            "  {label}: {old_pct:.1}% \u{2192} {new_pct:.1}% ({})",
            format!("\u{2191} {:.1}%", delta).green()
        );
    } else if delta < 0.0 {
        println!(
            "  {label}: {old_pct:.1}% \u{2192} {new_pct:.1}% ({})",
            format!("\u{2193} {:.1}%", delta.abs()).red()
        );
    } else {
        println!("  {label}: {new_pct:.1}% (unchanged)");
    }
}

#[cfg(test)]
mod tests {
    use super::{create_baseline, print_comparison};
    use crate::analyzer::{
        compute_severity, CallOccurrence, Classification, FunctionAnalysis, LogicOccurrence,
    };
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
        assert!((q - 1.0).abs() < f64::EPSILON);
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
}
