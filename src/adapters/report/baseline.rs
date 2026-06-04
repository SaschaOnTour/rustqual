use colored::Colorize;

use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};
use crate::domain::PERCENTAGE_MULTIPLIER;

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

    serde_json::to_string_pretty(&baseline)
        .unwrap_or_else(|e| format!("{{\"error\":\"baseline serialization failed: {e}\"}}"))
}

/// Build the v2-specific delta lines (TQ warnings, quality score) plus the
/// regression flag (quality score dropped). Pure so the TQ sum, the
/// `* PERCENTAGE_MULTIPLIER` scaling, and the regression comparison are
/// observable in tests rather than buried in stdout.
/// Operation: data extraction + comparison; own call hidden in closure.
pub(super) fn v2_delta_lines(raw: &serde_json::Value, summary: &Summary) -> (Vec<String>, bool) {
    let fmt = |label: &str, old_pct: f64, new_pct: f64| format_score_delta(label, old_pct, new_pct);
    let tq_keys = [
        "tq_no_assertion_warnings",
        "tq_no_sut_warnings",
        "tq_untested_warnings",
        "tq_uncovered_warnings",
        "tq_untested_logic_warnings",
    ];
    let old_tq: u64 = tq_keys.iter().map(|k| raw[*k].as_u64().unwrap_or(0)).sum();
    let new_tq = summary.tq_no_assertion_warnings
        + summary.tq_no_sut_warnings
        + summary.tq_untested_warnings
        + summary.tq_uncovered_warnings
        + summary.tq_untested_logic_warnings;
    let old_quality = raw["quality_score"].as_f64().unwrap_or(0.0);
    let lines = vec![
        format!(
            "  TQ warnings: {} \u{2192} {} ({:+})",
            old_tq,
            new_tq,
            new_tq as i64 - old_tq as i64
        ),
        fmt(
            "Quality",
            old_quality * PERCENTAGE_MULTIPLIER,
            summary.quality_score * PERCENTAGE_MULTIPLIER,
        ),
    ];
    (lines, summary.quality_score - old_quality < 0.0)
}

/// Build all baseline-comparison lines plus the regression flag, given the
/// already-parsed baseline JSON. Pure so the violation/finding deltas, the
/// IOSP scaling, and the v1/v2 regression branch are observable in tests.
/// Operation: comparison + display logic; own calls hidden in closures.
pub(super) fn comparison_lines(raw: &serde_json::Value, summary: &Summary) -> (Vec<String>, bool) {
    let fmt = |label: &str, old_pct: f64, new_pct: f64| format_score_delta(label, old_pct, new_pct);
    let v2 = |r: &serde_json::Value, s: &Summary| v2_delta_lines(r, s);
    let findings = |s: &Summary| s.total_findings();
    let is_v2 = raw.get("version").and_then(|v| v.as_u64()).unwrap_or(0) >= 2;
    let old_iosp = raw["iosp_score"].as_f64().unwrap_or(0.0);
    let old_violations = raw["violations"].as_u64().unwrap_or(0) as usize;
    let violation_delta = summary.violations as i64 - old_violations as i64;
    let mut lines = vec![
        format!(
            "\n{}",
            "\u{2550}\u{2550}\u{2550} Baseline Comparison \u{2550}\u{2550}\u{2550}".bold()
        ),
        format!(
            "  Violations: {} \u{2192} {} ({:+})",
            old_violations, summary.violations, violation_delta
        ),
    ];
    if is_v2 {
        let old_findings = raw["total_findings"].as_u64().unwrap_or(0) as usize;
        let finding_delta = findings(summary) as i64 - old_findings as i64;
        lines.push(format!(
            "  Findings:   {} \u{2192} {} ({:+})",
            old_findings,
            findings(summary),
            finding_delta
        ));
    }
    lines.push(fmt(
        "IOSP Score",
        old_iosp * PERCENTAGE_MULTIPLIER,
        summary.iosp_score * PERCENTAGE_MULTIPLIER,
    ));
    let regressed = if is_v2 {
        let (v2_lines, reg) = v2(raw, summary);
        lines.extend(v2_lines);
        reg
    } else {
        summary.iosp_score - old_iosp < 0.0
    };
    (lines, regressed)
}

/// Compare current results against a baseline and print delta.
/// Returns true if there was a regression (quality score decreased).
/// Supports v1 (IOSP-only) and v2 (full quality score) baseline formats.
/// Integration: parse, then delegate line-building to `comparison_lines`.
pub fn print_comparison(
    baseline_content: &str,
    _results: &[FunctionAnalysis],
    summary: &Summary,
) -> bool {
    let raw: serde_json::Value = match serde_json::from_str(baseline_content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing baseline: {e}");
            return false;
        }
    };
    let (lines, regressed) = comparison_lines(&raw, summary);
    lines.iter().for_each(|l| println!("{l}"));
    regressed
}

/// Build a labeled score-delta line: up-arrow (green) when the score rose,
/// down-arrow (red) when it fell, `(unchanged)` when equal. Pure so the
/// direction branch and the `new - old` delta are observable in tests.
/// Operation: conditional formatting logic, no own calls.
pub(super) fn format_score_delta(label: &str, old_pct: f64, new_pct: f64) -> String {
    let delta = new_pct - old_pct;
    if delta > 0.0 {
        format!(
            "  {label}: {old_pct:.1}% \u{2192} {new_pct:.1}% ({})",
            format!("\u{2191} {:.1}%", delta).green()
        )
    } else if delta < 0.0 {
        format!(
            "  {label}: {old_pct:.1}% \u{2192} {new_pct:.1}% ({})",
            format!("\u{2193} {:.1}%", delta.abs()).red()
        )
    } else {
        format!("  {label}: {new_pct:.1}% (unchanged)")
    }
}
