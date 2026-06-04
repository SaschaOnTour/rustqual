//! `app::exit_gates` + `app::setup` use-case tests: the suppression-ratio
//! warning builder, the per-gate `Result` checks, the composed
//! `apply_exit_gates` stack, and config-load error mapping. Split out of
//! `run.rs` to keep both test files under the test-file length baseline.
use crate::app::exit_gates::{
    apply_exit_gates, check_default_fail, check_fail_on_warnings, check_min_quality_score,
    check_quality_gates, suppression_ratio_warning,
};
use crate::cli::Cli;
use crate::config::Config;
use clap::Parser;

#[test]
fn test_check_fail_on_warnings_passes_when_no_warnings() {
    let mut config = Config::default();
    config.fail_on_warnings = true;
    let summary = crate::report::Summary {
        total: 10,
        ..Default::default()
    };
    assert!(check_fail_on_warnings(&config, &summary).is_ok());
}

#[test]
fn test_check_fail_on_warnings_passes_when_disabled() {
    let config = Config::default(); // fail_on_warnings = false
    let summary = crate::report::Summary {
        total: 10,
        suppression_ratio_exceeded: true,
        ..Default::default()
    };
    assert!(check_fail_on_warnings(&config, &summary).is_ok());
}

#[test]
fn test_check_fail_on_warnings_exits_when_triggered() {
    let mut config = Config::default();
    config.fail_on_warnings = true;
    let summary = crate::report::Summary {
        total: 10,
        suppression_ratio_exceeded: true,
        ..Default::default()
    };
    assert_eq!(check_fail_on_warnings(&config, &summary), Err(1));
}

#[test]
fn test_min_quality_score_cli_parse() {
    let cli = Cli::parse_from(["test", "--min-quality-score", "80.0"]);
    assert!((cli.min_quality_score.unwrap() - 80.0).abs() < f64::EPSILON);
}

#[test]
fn test_check_quality_gates_passes() {
    let cli = Cli::parse_from(["test", "--min-quality-score", "50.0"]);
    let mut summary = crate::report::Summary {
        total: 10,
        iosp_score: 1.0,
        ..Default::default()
    };
    summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
    assert!(check_quality_gates(&cli, &summary).is_ok());
}

#[test]
fn test_check_min_quality_score_below_threshold() {
    let mut summary = crate::report::Summary {
        total: 10,
        ..Default::default()
    };
    summary.quality_score = 0.5;
    assert_eq!(check_min_quality_score(90.0, &summary), Err(1));
}

#[test]
fn test_check_min_quality_score_above_threshold() {
    let mut summary = crate::report::Summary {
        total: 10,
        ..Default::default()
    };
    summary.quality_score = 0.95;
    assert!(check_min_quality_score(90.0, &summary).is_ok());
}

#[test]
fn test_check_quality_gates_below_threshold() {
    let cli = Cli::parse_from(["test", "--min-quality-score", "90.0"]);
    let summary = crate::report::Summary {
        total: 10,
        quality_score: 0.5,
        ..Default::default()
    };
    assert_eq!(check_quality_gates(&cli, &summary), Err(1));
}

#[test]
fn test_check_quality_gates_no_gate_set() {
    let cli = Cli::parse_from(["test"]);
    let summary = crate::report::Summary {
        total: 10,
        ..Default::default()
    };
    assert!(check_quality_gates(&cli, &summary).is_ok());
}

#[test]
fn test_check_min_quality_score_exactly_at_threshold_passes() {
    // The gate fails only *below* the floor; exactly at it passes. Pins the
    // `actual < min_score` comparison against `<=` (which would reject 80==80).
    let mut summary = crate::report::Summary {
        total: 10,
        ..Default::default()
    };
    summary.quality_score = 0.8; // 80.0% after the percentage multiplier
    assert!(
        check_min_quality_score(80.0, &summary).is_ok(),
        "score exactly at the floor must pass"
    );
}

#[test]
fn test_apply_exit_gates_returns_err_on_findings() {
    // End-to-end: a clean config + a summary carrying findings must make the
    // composed gate stack return Err(1) (via check_default_fail). Pins
    // `apply_exit_gates` against being replaced by `Ok(())`.
    let cli = Cli::parse_from(["test"]);
    let config = Config::default();
    let summary = crate::report::Summary {
        total: 10,
        violations: 3,
        ..Default::default()
    };
    assert_eq!(apply_exit_gates(&cli, &config, &summary), Err(1));
}

#[test]
fn test_suppression_ratio_warning_message_gating() {
    // The pure message builder returns Some only when the ratio is exceeded and
    // at least one function was analysed. The three rows pin every clause of the
    // `!exceeded || total == 0` guard: delete-`!`, `||`→`&&`, and `==`→`!=`.
    // (exceeded, total, expect_message)
    let cases = [(true, 10, true), (false, 10, false), (true, 0, false)];
    for (exceeded, total, expect) in cases {
        let summary = crate::report::Summary {
            total,
            all_suppressions: 4,
            suppression_ratio_exceeded: exceeded,
            ..Default::default()
        };
        assert_eq!(
            suppression_ratio_warning(&summary, 0.1).is_some(),
            expect,
            "exceeded={exceeded} total={total}"
        );
    }
}

#[test]
fn test_setup_config_missing_explicit_file_returns_exit_code_2() {
    // An explicit `--config` path that doesn't exist flows through
    // `load_explicit_config` → `report_config_error`, which prints to stderr
    // and maps to exit code 2. Pins `report_config_error`'s `2` (vs 0/-1/1) and
    // `load_explicit_config` against returning `Ok(Default::default())`.
    let cli = Cli::parse_from(["test", "--config", "/no/such/rustqual.toml"]);
    assert_eq!(crate::app::setup::setup_config(&cli).err(), Some(2));
}

#[test]
fn test_check_default_fail_with_findings() {
    assert_eq!(check_default_fail(false, 5), Err(1));
}

#[test]
fn test_check_default_fail_no_fail_mode() {
    assert!(check_default_fail(true, 5).is_ok());
}

#[test]
fn test_check_default_fail_no_findings() {
    assert!(check_default_fail(false, 0).is_ok());
}
