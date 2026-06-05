//! Per-kind targeted complexity suppression: `allow(complexity, <kind>[=N])`
//! silences exactly one kind (metric pins honour their value), leaving the
//! other complexity findings on the same function active.
use super::*;
use crate::domain::SuppressionTarget;
use crate::findings::{Dimension, Suppression};

fn targeted(name: &str, pin: Option<f64>) -> HashMap<String, Vec<Suppression>> {
    [(
        "test.rs".to_string(),
        vec![Suppression {
            line: 1,
            dimensions: vec![Dimension::Complexity],
            reason: Some("r".to_string()),
            target: Some(target_of(name, pin)),
        }],
    )]
    .into()
}

/// Build a metric or boolean target from an optional pin (test helper).
fn target_of(name: &str, pin: Option<f64>) -> SuppressionTarget {
    match pin {
        Some(pin) => SuppressionTarget::Metric {
            name: name.to_string(),
            pin,
        },
        None => SuppressionTarget::Boolean {
            name: name.to_string(),
        },
    }
}

fn run_extended(
    m: ComplexityMetrics,
    sups: &HashMap<String, Vec<Suppression>>,
) -> FunctionAnalysis {
    let mut results = vec![make_func_with_metrics(m)];
    let mut summary = Summary::from_results(&[]);
    apply_extended_warnings(
        &mut results,
        &Config::default(),
        &mut summary,
        &HashMap::new(),
        sups,
    );
    results.into_iter().next().unwrap()
}

fn run_complexity(
    m: ComplexityMetrics,
    sups: &HashMap<String, Vec<Suppression>>,
) -> FunctionAnalysis {
    let mut results = vec![make_func_with_metrics(m)];
    let mut summary = Summary::from_results(&[]);
    apply_complexity_warnings(&mut results, &Config::default(), &mut summary, sups);
    results.into_iter().next().unwrap()
}

#[test]
fn unsafe_target_suppresses_unsafe_not_nesting() {
    // unsafe is silenced by its boolean target; nesting on the same fn stays.
    let m = ComplexityMetrics {
        unsafe_blocks: 1,
        max_nesting: 5,
        ..Default::default()
    };
    let fa = run_extended(m, &targeted("unsafe", None));
    assert!(!fa.unsafe_warning, "unsafe should be silenced");
    assert!(fa.nesting_depth_warning, "nesting must still fire");
}

#[test]
fn cognitive_pin_suppresses_within_and_refires_above() {
    // Default max_cognitive = 15; pin at 18.
    let within = run_complexity(
        ComplexityMetrics {
            cognitive_complexity: 18,
            ..Default::default()
        },
        &targeted("max_cognitive", Some(18.0)),
    );
    assert!(!within.cognitive_warning, "18 <= pin 18 → silenced");
    let above = run_complexity(
        ComplexityMetrics {
            cognitive_complexity: 19,
            ..Default::default()
        },
        &targeted("max_cognitive", Some(18.0)),
    );
    assert!(above.cognitive_warning, "19 > pin 18 → re-fires");
}

#[test]
fn length_pin_suppresses_within() {
    let m = ComplexityMetrics {
        function_lines: 90,
        ..Default::default()
    };
    let fa = run_extended(m, &targeted("max_function_lines", Some(100.0)));
    assert!(!fa.function_length_warning, "90 <= pin 100 → silenced");
}

#[test]
fn wrong_target_does_not_suppress() {
    // A max_cognitive pin must not touch an unsafe finding.
    let m = ComplexityMetrics {
        unsafe_blocks: 1,
        ..Default::default()
    };
    let fa = run_extended(m, &targeted("max_cognitive", Some(99.0)));
    assert!(fa.unsafe_warning, "unsafe must still fire");
}
