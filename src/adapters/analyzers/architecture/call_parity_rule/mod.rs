//! Call-Parity check — cross-adapter delegation drift detection.
//!
//! Driven by `[architecture.call_parity]` + `[architecture.layers]`.
//! Two complementary checks under one rule:
//!
//! - **No-delegation (Check A)**: every `pub fn` in an adapter layer
//!   must transitively (up to `call_depth`) call into the target layer.
//! - **Missing-adapter (Check B)**: every `pub fn` in the target layer
//!   must be (transitively) reached from every adapter layer.
//!
//! Both walk a canonical call graph built from the workspace. Method
//! calls on receiver-bindings (`session.search(…)` when `session:
//! RlmSession`) resolve via `calls::collect_canonical_calls` so
//! Session/Service-pattern architectures aren't wholesale False-Positive.

mod bindings;
pub mod calls;
pub mod check_a;
pub mod check_b;
pub mod pub_fns;
pub mod workspace_graph;

use crate::adapters::analyzers::architecture::compiled::CompiledArchitecture;
use crate::adapters::analyzers::architecture::rendering::build_architecture_finding;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths_from_refs;
use crate::adapters::shared::use_tree::gather_alias_map;
use crate::domain::{Finding, Severity};
use crate::ports::AnalysisContext;
use std::collections::HashMap;

pub(crate) const RULE_NO_DELEGATION: &str = "architecture/call_parity/no_delegation";
pub(crate) const RULE_MISSING_ADAPTER: &str = "architecture/call_parity/missing_adapter";

/// Top-level entry for the architecture analyzer. Runs Check A + Check B
/// when `[architecture.call_parity]` is configured and projects raw
/// `MatchLocation`s into cross-dimension `Finding`s.
/// Integration: delegates graph build + per-check runs + projection.
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
    let pub_fns = pub_fns::collect_pub_fns_by_layer(&refs, &compiled.layers, &cfg_test_files);
    let graph = workspace_graph::build_call_graph(
        &refs,
        &aliases_per_file,
        &cfg_test_files,
        &compiled.layers,
    );
    let mut out = Vec::new();
    for hit in check_a::check_no_delegation(&pub_fns, &graph, &compiled.layers, cp) {
        out.push(project_call_parity(hit));
    }
    for hit in check_b::check_missing_adapter(&pub_fns, &graph, &compiled.layers, cp) {
        out.push(project_call_parity(hit));
    }
    out
}

fn project_call_parity(hit: MatchLocation) -> Finding {
    let rule_id = match &hit.kind {
        ViolationKind::CallParityNoDelegation { .. } => RULE_NO_DELEGATION,
        ViolationKind::CallParityMissingAdapter { .. } => RULE_MISSING_ADAPTER,
        _ => "architecture/call_parity",
    };
    build_architecture_finding(hit, rule_id.to_string(), "call parity", Severity::Medium)
}

#[cfg(test)]
mod tests;
