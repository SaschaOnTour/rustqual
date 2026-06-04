//! Target-aware suppression marking for the SRP dimension.
//!
//! A blanket `// qual:allow(srp)` silences every srp finding; a targeted
//! `allow(srp, <target>[=N])` silences exactly one finding-kind: `god_struct`
//! (struct), `file_length` / `max_independent_clusters` (module), or
//! `max_parameters` (param). Metric targets honour their pin value, so a
//! `file_length` pin can never hide a co-occurring cohesion finding.

use crate::findings::Suppression;

/// Mark SRP warnings as suppressed based on `// qual:allow(srp[, target])`
/// comments.
/// Operation: iteration + suppression matching via closures (no own calls).
pub(crate) fn mark_srp_suppressions(
    srp: Option<&mut crate::adapters::analyzers::srp::SrpAnalysis>,
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
    srp_config: &crate::config::sections::SrpConfig,
) {
    let Some(srp) = srp else { return };

    // Struct/param windows match the orphan detector, see
    // `app::suppression_windows::SRP_STRUCT_PARAM`.
    const WINDOW: usize = crate::app::suppression_windows::SRP_STRUCT_PARAM;
    let srp_dim = crate::findings::Dimension::Srp;
    let max_clusters = srp_config.max_independent_clusters;

    srp.struct_warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            w.suppressed = sups.iter().any(|sup| {
                let in_window = sup.line <= w.line && w.line - sup.line <= WINDOW;
                in_window && sup.suppresses(srp_dim, "god_struct", None)
            });
        }
    });

    srp.module_warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            w.suppressed = module_warning_suppressed(w, sups, srp_dim, max_clusters);
        }
    });

    srp.param_warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            let count = Some(w.parameter_count as f64);
            w.suppressed = sups.iter().any(|sup| {
                let in_window = sup.line <= w.line && w.line - sup.line <= WINDOW;
                in_window && sup.suppresses(srp_dim, "max_parameters", count)
            });
        }
    });
}

/// Decide whether a module SRP warning is fully suppressed. The warning can
/// have a length component (`production_lines > threshold`, recoverable from
/// `length_score > 1.0`) and/or a cohesion component (`clusters > max`); each
/// active component must be covered by a matching suppression, so a
/// `file_length` pin can never hide a co-occurring cohesion finding.
/// Operation: per-component coverage check via closures.
fn module_warning_suppressed(
    w: &crate::adapters::analyzers::srp::ModuleSrpWarning,
    sups: &[Suppression],
    srp_dim: crate::findings::Dimension,
    max_clusters: usize,
) -> bool {
    let has_length = w.length_score > 1.0;
    let has_cluster = w.independent_clusters > max_clusters;
    let lines = Some(w.production_lines as f64);
    let clusters = Some(w.independent_clusters as f64);
    let length_ok = !has_length
        || sups
            .iter()
            .any(|s| s.suppresses(srp_dim, "file_length", lines));
    let cluster_ok = !has_cluster
        || sups
            .iter()
            .any(|s| s.suppresses(srp_dim, "max_independent_clusters", clusters));
    length_ok && cluster_ok
}
