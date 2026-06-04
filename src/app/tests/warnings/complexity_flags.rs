//! `apply_complexity_warnings` threshold flags. Metrics are placed exactly at
//! or one over `max_cognitive`/`max_cyclomatic` (both set to 5) so the `>`
//! comparisons, the `||` between them, and the `+=` counters all flip.
use super::*;
use crate::adapters::analyzers::iosp::MagicNumberOccurrence;

fn cx_config(max_cognitive: usize, max_cyclomatic: usize) -> Config {
    let mut c = Config::default();
    c.complexity.max_cognitive = max_cognitive;
    c.complexity.max_cyclomatic = max_cyclomatic;
    c
}

/// `(cognitive_warning, cyclomatic_warning, complexity_warnings, magic_warnings)`.
fn apply_cx(metrics: ComplexityMetrics, config: &Config) -> (bool, bool, usize, usize) {
    let mut fa = make_func_with_metrics(metrics);
    let mut summary = Summary::from_results(&[]);
    apply_complexity_warnings(std::slice::from_mut(&mut fa), config, &mut summary);
    (
        fa.cognitive_warning,
        fa.cyclomatic_warning,
        summary.complexity_warnings,
        summary.magic_number_warnings,
    )
}

#[test]
fn complexity_exactly_at_threshold_does_not_warn() {
    // cognitive == max AND cyclomatic == max → neither over → no warning.
    let m = ComplexityMetrics {
        cognitive_complexity: 5,
        cyclomatic_complexity: 5,
        ..Default::default()
    };
    assert_eq!(apply_cx(m, &cx_config(5, 5)), (false, false, 0, 0));
}

#[test]
fn cognitive_over_threshold_warns_alone() {
    // cognitive over, cyclomatic at threshold → cognitive_warning only, counted
    // once. Guards the `||` (an `&&` would need both over) and the `+= 1`.
    let m = ComplexityMetrics {
        cognitive_complexity: 6,
        cyclomatic_complexity: 5,
        ..Default::default()
    };
    assert_eq!(apply_cx(m, &cx_config(5, 5)), (true, false, 1, 0));
}

#[test]
fn cyclomatic_over_threshold_warns_alone() {
    let m = ComplexityMetrics {
        cognitive_complexity: 5,
        cyclomatic_complexity: 6,
        ..Default::default()
    };
    assert_eq!(apply_cx(m, &cx_config(5, 5)), (false, true, 1, 0));
}

#[test]
fn magic_numbers_are_summed() {
    // A non-test function's magic numbers are added to the summary; guards the
    // `magic_number_warnings += m.magic_numbers.len()`.
    let m = ComplexityMetrics {
        magic_numbers: vec![
            MagicNumberOccurrence {
                line: 1,
                value: "42".into(),
            },
            MagicNumberOccurrence {
                line: 2,
                value: "7".into(),
            },
            MagicNumberOccurrence {
                line: 3,
                value: "13".into(),
            },
        ],
        ..Default::default()
    };
    assert_eq!(apply_cx(m, &cx_config(5, 5)).3, 3);
}

#[test]
fn disabled_complexity_emits_no_warnings() {
    // With complexity disabled the pass is a no-op even for wildly-over metrics;
    // guards the `!config.complexity.enabled` early-return.
    let mut config = cx_config(5, 5);
    config.complexity.enabled = false;
    let m = ComplexityMetrics {
        cognitive_complexity: 99,
        cyclomatic_complexity: 99,
        ..Default::default()
    };
    assert_eq!(apply_cx(m, &config), (false, false, 0, 0));
}
