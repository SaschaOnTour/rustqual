//! Module-keyed, target-aware coupling suppression.
//!
//! Coupling markers are MODULE-GLOBAL: `// qual:allow(coupling[, target])` in
//! any file of a module applies to that whole module. A blanket allow(coupling)
//! silences every coupling finding for the module; targeted forms silence one
//! kind: `max_fan_in` / `max_fan_out` / `max_instability` (metric pins) or
//! `sdp` (boolean). Cycles are never suppressible.

use std::collections::HashMap;

use crate::adapters::analyzers::coupling::CouplingAnalysis;
use crate::adapters::shared::file_to_module::file_to_module;
use crate::findings::{Dimension, Suppression};

/// Coupling suppressions grouped by the module they apply to.
pub(crate) struct ModuleCouplingSuppressions {
    by_module: HashMap<String, Vec<Suppression>>,
}

impl ModuleCouplingSuppressions {
    /// Build from per-file suppression lines, keeping only coupling markers and
    /// mapping each file to its module.
    /// Operation: grouping via closures.
    pub(crate) fn build(suppression_lines: &HashMap<String, Vec<Suppression>>) -> Self {
        let mut by_module: HashMap<String, Vec<Suppression>> = HashMap::new();
        suppression_lines.iter().for_each(|(path, sups)| {
            let module = file_to_module(path);
            sups.iter()
                .filter(|s| s.covers(Dimension::Coupling))
                .for_each(|s| {
                    by_module.entry(module.clone()).or_default().push(s.clone());
                });
        });
        Self { by_module }
    }

    /// Whether `module` has a blanket (untargeted) coupling suppression.
    /// Operation: lookup + any.
    pub(crate) fn blanket(&self, module: &str) -> bool {
        self.by_module
            .get(module)
            .is_some_and(|v| v.iter().any(|s| s.target.is_none()))
    }

    /// Whether `module`'s `target` finding at `value` is silenced (blanket or a
    /// matching targeted pin).
    /// Operation: lookup + any.
    pub(crate) fn suppresses(&self, module: &str, target: &str, value: Option<f64>) -> bool {
        self.by_module.get(module).is_some_and(|v| {
            v.iter()
                .any(|s| s.suppresses(Dimension::Coupling, target, value))
        })
    }
}

/// Mark each module's blanket suppression flag (used for SDP inheritance and
/// leaf handling). Targeted metric/sdp suppression is applied at count time.
/// Operation: iterates metrics setting the blanket flag.
pub(crate) fn mark_coupling_blanket(
    analysis: Option<&mut CouplingAnalysis>,
    sups: &ModuleCouplingSuppressions,
) {
    let Some(analysis) = analysis else { return };
    analysis.metrics.iter_mut().for_each(|m| {
        if sups.blanket(&m.module_name) {
            m.suppressed = true;
        }
    });
}

/// Suppress SDP violations whose source/target module carries a targeted
/// `allow(coupling, sdp)` (blanket suppression is already inherited in
/// `populate_sdp_violations`).
/// Operation: iterates violations applying the sdp target.
pub(crate) fn mark_sdp_target_suppressions(
    analysis: &mut CouplingAnalysis,
    sups: &ModuleCouplingSuppressions,
) {
    analysis.sdp_violations.iter_mut().for_each(|v| {
        if sups.suppresses(&v.from_module, "sdp", None)
            || sups.suppresses(&v.to_module, "sdp", None)
        {
            v.suppressed = true;
        }
    });
}
