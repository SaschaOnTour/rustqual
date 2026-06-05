use crate::config::Config;
use crate::findings::{Dimension, Suppression};
use crate::report::Summary;

/// Compute structural analysis if enabled.
/// Operation: conditional check + module-qualified call.
pub(super) fn compute_structural(
    parsed: &[(String, String, syn::File)],
    config: &Config,
) -> Option<crate::adapters::analyzers::structural::StructuralAnalysis> {
    if !config.structural.enabled {
        return None;
    }
    Some(crate::adapters::analyzers::structural::analyze_structural(
        parsed,
        &config.structural,
    ))
}

/// Mark structural warnings as suppressed based on suppression comments.
/// Operation: iteration + suppression matching, no own calls.
/// Uses the warning's dimension (SRP or Coupling) to match suppressions.
///
/// Each structural check has its own boolean suppression target named by its
/// lowercased code (`oi`/`sit`/`deh`/`iet` on the coupling side,
/// `btc`/`slm`/`nms` on the SRP side), so a genuinely-unfixable finding (an
/// orphan-rule-forced impl, a single-impl API boundary, a `downcast` plugin
/// seam) can be silenced by name: `// qual:allow(coupling, oi) reason: "…"`.
/// A targeted marker silences only its own kind — `allow(coupling, sdp)` does
/// not touch an `OI` finding — and a metric coupling pin never silences a
/// structural one (its `value` is `None`). This goes through `suppresses` so
/// the marking and orphan-detection layers stay in lockstep.
pub(super) fn mark_structural_suppressions(
    structural: Option<&mut crate::adapters::analyzers::structural::StructuralAnalysis>,
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
) {
    let Some(structural) = structural else { return };
    // Window width shared with the orphan detector, see
    // `app::suppression_windows::STRUCTURAL`.
    let window = super::suppression_windows::STRUCTURAL;
    structural.warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            w.suppressed = sups.iter().any(|sup| {
                let in_window = sup.line <= w.line && w.line - sup.line <= window;
                in_window && sup.suppresses(w.dimension, w.kind.target_name(), None)
            });
        }
    });
}

/// Count structural warnings and update summary, excluding suppressed entries.
/// Operation: iteration + conditional counting by dimension, no own calls.
pub(super) fn count_structural_warnings(
    structural: Option<&crate::adapters::analyzers::structural::StructuralAnalysis>,
    summary: &mut Summary,
) {
    let Some(structural) = structural else { return };
    structural
        .warnings
        .iter()
        .filter(|w| !w.suppressed)
        .for_each(|w| match w.dimension {
            Dimension::Srp => summary.structural_srp_warnings += 1,
            Dimension::Coupling => summary.structural_coupling_warnings += 1,
            _ => {}
        });
}
