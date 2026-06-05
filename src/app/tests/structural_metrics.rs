//! `app::structural_metrics` — enable guard, suppression window (STRUCTURAL = 5)
//! and per-dimension warning counting.
use crate::adapters::analyzers::structural::{
    StructuralAnalysis, StructuralWarning, StructuralWarningKind,
};
use crate::app::structural_metrics::*;
use crate::config::Config;
use crate::findings::Dimension;
use crate::report::Summary;

fn warning(line: usize, dim: Dimension, suppressed: bool) -> StructuralWarning {
    StructuralWarning {
        file: "test.rs".to_string(),
        line,
        name: "m".to_string(),
        kind: StructuralWarningKind::SelflessMethod,
        dimension: dim,
        suppressed,
    }
}

#[test]
fn compute_structural_returns_some_when_enabled() {
    // Guards the `!config.structural.enabled` early-return.
    assert!(compute_structural(&[], &Config::default()).is_some());
}

fn struct_suppressed(w_line: usize, sup_line: usize, w_dim: Dimension, sup_dim: Dimension) -> bool {
    let mut analysis = StructuralAnalysis {
        warnings: vec![warning(w_line, w_dim, false)],
    };
    let sups = super::one_suppression(sup_line, sup_dim);
    mark_structural_suppressions(Some(&mut analysis), &sups);
    analysis.warnings[0].suppressed
}

#[test]
fn structural_suppression_window_and_dimension() {
    let s = Dimension::Srp;
    // same line, in window
    assert!(struct_suppressed(5, 5, s, s));
    // diff 5 == STRUCTURAL window
    assert!(struct_suppressed(6, 1, s, s));
    // diff 6 > window
    assert!(!struct_suppressed(7, 1, s, s));
    // in window but covers a different dimension (kills the `&& covers`)
    assert!(!struct_suppressed(5, 5, s, Dimension::Complexity));
    // diff 8 > window — kills `w.line - sup.line` `-`→`/` (10/2=5 ≤ 5)
    assert!(!struct_suppressed(10, 2, s, s));
    // suppression below the warning (kills the leading `&&` / underflow)
    assert!(!struct_suppressed(2, 5, s, s));
}

#[test]
fn unrelated_targeted_marker_does_not_suppress_structural_warning() {
    // A targeted marker silences only its OWN target. The warning is an SLM
    // finding (target `slm`); an `allow(coupling, sdp)` marker — valid, covering
    // the dimension, in-window — names a different target, so it must not
    // silence it. (Before the fix, the dimension-only `covers()` check let any
    // targeted marker hide structural findings.)
    let mut analysis = StructuralAnalysis {
        warnings: vec![warning(5, Dimension::Coupling, false)],
    };
    let sups = [(
        "test.rs".to_string(),
        vec![crate::findings::Suppression {
            line: 5,
            dimensions: vec![Dimension::Coupling],
            reason: Some("r".into()),
            target: Some(crate::domain::SuppressionTarget::Boolean { name: "sdp".into() }),
        }],
    )]
    .into();
    mark_structural_suppressions(Some(&mut analysis), &sups);
    assert!(
        !analysis.warnings[0].suppressed,
        "a targeted coupling marker for a different target must not hide a structural finding"
    );
}

fn structural_target(
    line: usize,
    dim: Dimension,
    target: &str,
) -> std::collections::HashMap<String, Vec<crate::findings::Suppression>> {
    [(
        "test.rs".to_string(),
        vec![crate::findings::Suppression {
            line,
            dimensions: vec![dim],
            reason: Some("r".into()),
            target: Some(crate::domain::SuppressionTarget::Boolean {
                name: target.into(),
            }),
        }],
    )]
    .into()
}

#[test]
fn structural_target_suppresses_its_own_kind() {
    // `warning(..)` is an SLM finding → `allow(srp, slm)` silences it.
    let mut a = StructuralAnalysis {
        warnings: vec![warning(5, Dimension::Srp, false)],
    };
    mark_structural_suppressions(Some(&mut a), &structural_target(5, Dimension::Srp, "slm"));
    assert!(
        a.warnings[0].suppressed,
        "allow(srp, slm) must silence an SLM finding"
    );
}

#[test]
fn structural_target_does_not_suppress_a_different_kind() {
    // `allow(srp, btc)` must NOT silence an SLM finding.
    let mut a = StructuralAnalysis {
        warnings: vec![warning(5, Dimension::Srp, false)],
    };
    mark_structural_suppressions(Some(&mut a), &structural_target(5, Dimension::Srp, "btc"));
    assert!(
        !a.warnings[0].suppressed,
        "allow(srp, btc) must not silence an SLM finding"
    );
}

#[test]
fn count_structural_warnings_splits_by_dimension() {
    // SRP: 2 unsuppressed + 1 suppressed → 2. Coupling: 1 unsuppressed + 1
    // suppressed → 1. Distinct counts pin the `!suppressed` filter, both match
    // arms and their `+= 1`, and the function against being a no-op.
    let analysis = StructuralAnalysis {
        warnings: vec![
            warning(1, Dimension::Srp, false),
            warning(2, Dimension::Srp, false),
            warning(3, Dimension::Srp, true),
            warning(4, Dimension::Coupling, false),
            warning(5, Dimension::Coupling, true),
        ],
    };
    let mut summary = Summary::from_results(&[]);
    count_structural_warnings(Some(&analysis), &mut summary);
    assert_eq!(summary.structural_srp_warnings, 2);
    assert_eq!(summary.structural_coupling_warnings, 1);
}
