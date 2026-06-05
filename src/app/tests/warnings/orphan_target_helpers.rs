//! Shared helpers for the targeted-orphan tests (`orphan_too_loose` +
//! `orphan_targeting`), kept in one place so each test file stays under the
//! SRP file-length cap.

use super::*;
use crate::domain::findings::{OrphanSuppression, SrpFinding, SrpFindingDetails};
use crate::domain::SuppressionTarget;
use crate::findings::{Dimension, Suppression};

/// Build a metric or boolean target from an optional pin.
pub(super) fn target_of(name: &str, pin: Option<f64>) -> SuppressionTarget {
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

/// Run the orphan detector for one targeted marker on `src/x.rs` (default cfg).
pub(super) fn orphans(
    dim: Dimension,
    target: &str,
    pin: Option<f64>,
    sup_line: usize,
    seed: impl FnMut(&mut crate::report::AnalysisResult),
) -> Vec<OrphanSuppression> {
    orphans_cfg(
        target_of(target, pin),
        dim,
        sup_line,
        &Config::default(),
        seed,
    )
}

/// Like `orphans`, but with a pre-built target and an explicit config (so
/// `pin_headroom` overrides and per-dimension enables can be exercised).
pub(super) fn orphans_cfg(
    target: SuppressionTarget,
    dim: Dimension,
    sup_line: usize,
    config: &Config,
    mut seed: impl FnMut(&mut crate::report::AnalysisResult),
) -> Vec<OrphanSuppression> {
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: sup_line,
            dimensions: vec![dim],
            reason: Some("r".to_string()),
            target: Some(target),
        }],
    );
    let mut analysis = empty_analysis();
    seed(&mut analysis);
    crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &std::collections::HashMap::new(),
        &analysis,
        config,
    )
}

/// A module-length finding at `production_lines` lines (length component
/// active, cohesion inactive — inherited from `make_srp_module_finding`).
pub(super) fn srp_module(file: &str, production_lines: usize) -> SrpFinding {
    let mut f = make_srp_module_finding(file);
    if let SrpFindingDetails::ModuleLength {
        production_lines: pl,
        ..
    } = &mut f.details
    {
        *pl = production_lines;
    }
    f
}

/// A module-length finding with explicit length/cohesion activity, so the
/// component-gated position emission can be exercised: `length_score > 1.0`
/// activates the `file_length` target; `clusters > max (2)` activates the
/// `max_independent_clusters` target.
pub(super) fn srp_module_full(
    file: &str,
    production_lines: usize,
    length_score: f64,
    clusters: usize,
) -> SrpFinding {
    let mut f = make_srp_module_finding(file);
    if let SrpFindingDetails::ModuleLength {
        production_lines: pl,
        independent_clusters: ic,
        length_score: ls,
        ..
    } = &mut f.details
    {
        *pl = production_lines;
        *ic = clusters;
        *ls = length_score;
    }
    f
}
