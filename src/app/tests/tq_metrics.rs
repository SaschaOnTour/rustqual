//! `app::tq_metrics` — suppression window (TQ = 5) and per-kind warning counting.
use crate::adapters::analyzers::tq::{TqAnalysis, TqWarning, TqWarningKind};
use crate::app::tq_metrics::*;
use crate::findings::Dimension;
use crate::report::Summary;

fn warning(line: usize, kind: TqWarningKind, suppressed: bool) -> TqWarning {
    TqWarning {
        file: "test.rs".to_string(),
        line,
        function_name: "t".to_string(),
        kind,
        suppressed,
    }
}

fn tq_suppressed(w_line: usize, sup_line: usize, dim: Dimension) -> bool {
    let mut tq = TqAnalysis {
        warnings: vec![warning(w_line, TqWarningKind::NoAssertion, false)],
    };
    let sups = super::one_suppression(sup_line, dim);
    mark_tq_suppressions(Some(&mut tq), &sups);
    tq.warnings[0].suppressed
}

#[test]
fn tq_suppression_window_and_dimension() {
    let t = Dimension::TestQuality;
    assert!(tq_suppressed(5, 5, t)); // same line
    assert!(tq_suppressed(6, 1, t)); // diff 5 == TQ window
    assert!(!tq_suppressed(7, 1, t)); // diff 6 > window
    assert!(!tq_suppressed(5, 5, Dimension::Complexity)); // wrong dimension
    assert!(!tq_suppressed(10, 2, t)); // diff 8 > window (kills `-`→`/`: 10/2=5≤5)
    assert!(!tq_suppressed(2, 5, t)); // below the warning
}

fn tq_targeted(w_line: usize, kind: TqWarningKind, target: &str) -> bool {
    let mut tq = TqAnalysis {
        warnings: vec![warning(w_line, kind, false)],
    };
    let sups: std::collections::HashMap<String, Vec<crate::findings::Suppression>> = [(
        "test.rs".to_string(),
        vec![crate::findings::Suppression {
            line: w_line,
            dimensions: vec![Dimension::TestQuality],
            reason: Some("r".to_string()),
            target: Some(crate::domain::SuppressionTarget::Boolean {
                name: target.to_string(),
            }),
        }],
    )]
    .into();
    mark_tq_suppressions(Some(&mut tq), &sups);
    tq.warnings[0].suppressed
}

#[test]
fn tq_targeted_suppression_is_per_kind() {
    // A no_assertion target silences a NoAssertion warning, but not another kind.
    assert!(tq_targeted(5, TqWarningKind::NoAssertion, "no_assertion"));
    assert!(!tq_targeted(5, TqWarningKind::NoAssertion, "untested"));
    // An untested target silences an Untested warning.
    assert!(tq_targeted(5, TqWarningKind::Untested, "untested"));
}

#[test]
fn count_tq_warnings_splits_by_kind() {
    // One unsuppressed warning of each kind → each summary counter is 1. Pins
    // the per-kind `+= 1` (a `*=`/`-=` would give 0) and the no-op mutant.
    let tq = TqAnalysis {
        warnings: vec![
            warning(1, TqWarningKind::NoAssertion, false),
            warning(2, TqWarningKind::NoSut, false),
            warning(3, TqWarningKind::Untested, false),
            warning(4, TqWarningKind::Uncovered, false),
            warning(
                5,
                TqWarningKind::UntestedLogic {
                    uncovered_lines: vec![("test.rs".to_string(), 5)],
                },
                false,
            ),
        ],
    };
    let mut summary = Summary::from_results(&[]);
    count_tq_warnings(Some(&tq), &mut summary);
    assert_eq!(summary.tq_no_assertion_warnings, 1);
    assert_eq!(summary.tq_no_sut_warnings, 1);
    assert_eq!(summary.tq_untested_warnings, 1);
    assert_eq!(summary.tq_uncovered_warnings, 1);
    assert_eq!(summary.tq_untested_logic_warnings, 1);
}
