//! Use-case: translate an analysis outcome into the process's exit code.
//!
//! `apply_exit_gates` is the single function the composition root calls
//! after every run. It emits the suppression-ratio warning (stderr only),
//! then walks the explicit gates: `--fail-on-warnings`, `--min-quality-score`,
//! and the default-fail guard (`--no-fail` inverts it). The first
//! violated gate returns `Err(1)`; everything else returns `Ok(())`.

use crate::cli::Cli;
use crate::config::Config;
use crate::domain::PERCENTAGE_MULTIPLIER;
use crate::report::Summary;

/// Apply all exit gates in order.
/// Integration: delegates to warning + per-gate checks.
pub(crate) fn apply_exit_gates(cli: &Cli, config: &Config, summary: &Summary) -> Result<(), i32> {
    warn_suppression_ratio(summary, config.max_suppression_ratio);
    check_fail_on_warnings(config, summary)?;
    check_quality_gates(cli, summary)?;
    check_default_fail(cli.no_fail, summary.total_findings())
}

/// Emit a stderr warning when the suppression ratio exceeds the configured max.
/// Operation: conditional formatting.
fn warn_suppression_ratio(summary: &Summary, max_ratio: f64) {
    if !summary.suppression_ratio_exceeded || summary.total == 0 {
        return;
    }
    eprintln!(
        "Warning: {} suppression(s) found ({:.1}% of functions, max: {:.1}%)",
        summary.all_suppressions,
        summary.all_suppressions as f64 / summary.total as f64 * PERCENTAGE_MULTIPLIER,
        max_ratio * PERCENTAGE_MULTIPLIER,
    );
}

/// Return Err(1) iff `--fail-on-warnings` is set and there are warnings.
/// Orphan `qual:allow` markers are emitted as ORPHAN_SUPPRESSION
/// findings and handled through the normal finding-count path.
/// Operation: conditional check.
pub(crate) fn check_fail_on_warnings(config: &Config, summary: &Summary) -> Result<(), i32> {
    if config.fail_on_warnings && summary.suppression_ratio_exceeded {
        eprintln!("Error: warnings present and --fail-on-warnings is set");
        return Err(1);
    }
    Ok(())
}

/// Run each configured quality gate (currently only `--min-quality-score`).
/// Integration: dispatches to check_min_quality_score.
pub(crate) fn check_quality_gates(cli: &Cli, summary: &Summary) -> Result<(), i32> {
    cli.min_quality_score
        .iter()
        .try_for_each(|&s| check_min_quality_score(s, summary))
}

/// Return Err(1) iff the quality score is below `min_score`.
/// Operation: conditional check.
pub(crate) fn check_min_quality_score(min_score: f64, summary: &Summary) -> Result<(), i32> {
    let actual = summary.quality_score * PERCENTAGE_MULTIPLIER;
    if actual < min_score {
        eprintln!("Quality score {actual:.1}% is below minimum {min_score:.1}%");
        return Err(1);
    }
    Ok(())
}

/// Return Err(1) iff there are findings and `--no-fail` was not passed.
/// Operation: conditional check.
pub(crate) fn check_default_fail(no_fail: bool, total_findings: usize) -> Result<(), i32> {
    if !no_fail && total_findings > 0 {
        return Err(1);
    }
    Ok(())
}
