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
/// Structural checks (BTC/SLM/NMS on the SRP side, OI/SIT/DEH/IET on the
/// coupling side) are NOT part of either dimension's suppression-target
/// vocabulary, so only a *blanket* marker (`target.is_none()`) may silence
/// them — a targeted marker like `allow(coupling, sdp)` silences its own
/// finding-kind, not an unrelated structural one. This mirrors the orphan
/// detector, where structural positions carry `target: None` and so match
/// blanket markers only. Since blanket SRP/Coupling markers are rejected by
/// the parser ("the flip"), a structural finding is effectively unsuppressible.
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
                in_window && sup.target.is_none() && sup.covers(w.dimension)
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
