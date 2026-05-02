//! Call-Parity check — cross-adapter delegation drift detection.
//!
//! Driven by `[architecture.call_parity]` + `[architecture.layers]`.
//! Four checks anchored at the boundary between adapter and target
//! layers (the first call from an adapter into the target):
//!
//! - **A — No-delegation**: every adapter `pub fn` must reach the
//!   target layer. Empty touchpoint set ⇒ finding.
//! - **B — Missing adapter**: a target `pub fn` reached by some
//!   adapter must be reached by every adapter, OR be transitively
//!   reachable from some adapter touchpoint via target-internal
//!   callers (otherwise it's an orphan / dead target-layer island
//!   and gets flagged).
//! - **C — Single touchpoint**: each adapter `pub fn` should reach
//!   exactly one target node (configurable via `single_touchpoint`).
//! - **D — Multiplicity match**: a target reached by every adapter
//!   must be reached with the same handler count from each.
//!
//! All four checks read from a shared `HandlerTouchpoints` cache
//! built once via `build_handler_touchpoints` (forward BFS per
//! adapter pub-fn, stops on first target-layer hit). Method calls
//! on receiver-bindings (`session.search(…)` when `session:
//! RlmSession`) resolve via `calls::collect_canonical_calls` so
//! Session/Service-pattern architectures aren't wholesale False-Positive.

mod bindings;
pub mod calls;
pub mod check_a;
pub mod check_b;
pub mod check_c;
pub mod check_d;
pub(crate) mod local_symbols;
pub mod pub_fns;
mod pub_fns_alias_chain;
mod pub_fns_use_tree;
mod pub_fns_visibility;
pub(crate) mod signature_params;
pub mod touchpoints;
pub mod type_infer;
pub mod workspace_graph;

use crate::adapters::analyzers::architecture::compiled::{
    CompiledArchitecture, CompiledCallParity,
};
use crate::adapters::analyzers::architecture::rendering::build_architecture_finding;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths_from_refs;
use crate::adapters::shared::use_tree::gather_alias_map;
use crate::config::architecture::SingleTouchpointMode;
use crate::domain::{Finding, Severity};
use crate::ports::AnalysisContext;
use pub_fns::PubFnInfo;
use std::collections::{HashMap, HashSet};
use touchpoints::{compute_touchpoints, TouchpointContext};
use workspace_graph::{canonical_name_for_pub_fn, CallGraph};

pub(crate) const RULE_NO_DELEGATION: &str = "architecture/call_parity/no_delegation";
pub(crate) const RULE_MISSING_ADAPTER: &str = "architecture/call_parity/missing_adapter";
pub(crate) const RULE_MULTIPLICITY_MISMATCH: &str =
    "architecture/call_parity/multiplicity_mismatch";
pub(crate) const RULE_MULTI_TOUCHPOINT: &str = "architecture/call_parity/multi_touchpoint";

/// Top-level entry for the architecture analyzer. Runs Checks A/B/C/D
/// when `[architecture.call_parity]` is configured and projects raw
/// `MatchLocation`s into cross-dimension `Finding`s.
/// Integration: delegates graph build + touchpoint cache + per-check
/// runs + projection.
pub fn collect_findings(
    ctx: &AnalysisContext<'_>,
    compiled: &CompiledArchitecture,
) -> Vec<Finding> {
    let Some(cp) = compiled.call_parity.as_ref() else {
        return Vec::new();
    };
    let refs: Vec<(&str, &syn::File)> = ctx
        .files
        .iter()
        .map(|f| (f.path.as_str(), &f.ast))
        .collect();
    let cfg_test_files = collect_cfg_test_file_paths_from_refs(&refs);
    let aliases_per_file: HashMap<String, HashMap<String, Vec<String>>> = refs
        .iter()
        .map(|(p, f)| (p.to_string(), gather_alias_map(f)))
        .collect();
    let pub_fns = pub_fns::collect_pub_fns_by_layer(
        &refs,
        &aliases_per_file,
        &compiled.layers,
        &cfg_test_files,
        &cp.transparent_wrappers,
    );
    let graph = workspace_graph::build_call_graph(
        &refs,
        &aliases_per_file,
        &cfg_test_files,
        &compiled.layers,
        &cp.transparent_wrappers,
    );
    let touchpoints = build_handler_touchpoints(&pub_fns, &graph, cp);
    let mut out = Vec::new();
    for hit in check_a::check_no_delegation(&pub_fns, &touchpoints, cp) {
        out.push(project_call_parity(hit, cp));
    }
    for hit in check_b::check_missing_adapter(&pub_fns, &graph, &touchpoints, cp) {
        out.push(project_call_parity(hit, cp));
    }
    for hit in check_c::check_multi_touchpoint(&pub_fns, &touchpoints, cp) {
        out.push(project_call_parity(hit, cp));
    }
    for hit in check_d::check_multiplicity_mismatch(&pub_fns, &touchpoints, cp) {
        out.push(project_call_parity(hit, cp));
    }
    out
}

/// Per-adapter-handler touchpoint cache: maps each non-deprecated
/// adapter pub-fn's canonical name to the set of target-layer
/// canonicals it touches at the boundary. Built once per analysis
/// run and shared across all four checks (A/B/C/D) so each adapter
/// handler pays for one BFS rather than four.
pub(crate) type HandlerTouchpoints = HashMap<String, HashSet<String>>;

// qual:api
/// Compute the per-handler touchpoint cache. Skips deprecated
/// handlers up front so checks needn't re-filter. The BFS walks
/// are independent and run in parallel via rayon — each handler's
/// `compute_touchpoints` only reads from the shared `&CallGraph`
/// (HashMap of Strings, no `syn`-bound state, so no Sync issue).
/// Integration: collect active canonicals + delegate parallel BFS.
pub(crate) fn build_handler_touchpoints(
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'_>>>,
    graph: &CallGraph,
    cp: &CompiledCallParity,
) -> HandlerTouchpoints {
    use rayon::prelude::*;
    let canonicals = collect_active_handler_canonicals(pub_fns_by_layer, cp);
    canonicals
        .into_par_iter()
        .map(|(canonical, origin_adapter)| {
            let ctx = TouchpointContext {
                graph,
                target_layer: &cp.target,
                call_depth: cp.call_depth,
                origin_adapter: &origin_adapter,
                adapter_layers: &cp.adapters,
            };
            let tps = compute_touchpoints(&canonical, &ctx);
            (canonical, tps)
        })
        .collect()
}

/// Collect the canonical names of every non-deprecated adapter
/// pub-fn. `compile_call_parity` enforces that adapter layers are
/// disjoint and `collect_pub_fns_by_layer` files each fn under one
/// layer, so each canonical appears at most once across the result.
/// Operation: nested fold.
fn collect_active_handler_canonicals(
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'_>>>,
    cp: &CompiledCallParity,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for adapter in &cp.adapters {
        let Some(handlers) = pub_fns_by_layer.get(adapter) else {
            continue;
        };
        for info in handlers {
            if info.deprecated {
                continue;
            }
            out.push((canonical_name_for_pub_fn(info), adapter.clone()));
        }
    }
    out
}

fn project_call_parity(hit: MatchLocation, cp: &CompiledCallParity) -> Finding {
    let rule_id = match &hit.kind {
        ViolationKind::CallParityNoDelegation { .. } => RULE_NO_DELEGATION,
        ViolationKind::CallParityMissingAdapter { .. } => RULE_MISSING_ADAPTER,
        ViolationKind::CallParityMultiplicityMismatch { .. } => RULE_MULTIPLICITY_MISMATCH,
        ViolationKind::CallParityMultiTouchpoint { .. } => RULE_MULTI_TOUCHPOINT,
        _ => "architecture/call_parity",
    };
    let severity = severity_for(&hit.kind, cp);
    build_architecture_finding(hit, rule_id.to_string(), "call parity", severity)
}

/// Pick severity per rule. Check C's severity follows `single_touchpoint`
/// (Warn → Low, Error → Medium; Off is filtered out before projection).
/// All other call-parity findings are Medium.
/// Operation: variant dispatch.
fn severity_for(kind: &ViolationKind, cp: &CompiledCallParity) -> Severity {
    match kind {
        ViolationKind::CallParityMultiTouchpoint { .. } => match cp.single_touchpoint {
            SingleTouchpointMode::Error => Severity::Medium,
            SingleTouchpointMode::Warn | SingleTouchpointMode::Off => Severity::Low,
        },
        _ => Severity::Medium,
    }
}

#[cfg(test)]
mod tests;
