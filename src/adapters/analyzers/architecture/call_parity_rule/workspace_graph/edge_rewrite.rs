//! Post-build pass that rewrites phantom call-graph edges to the
//! correct trait anchor.
//!
//! A UFCS call like `AppHandler::handle(&x)` against
//! `impl Handler for AppHandler {}` (no override, default body on the
//! trait) emits an edge to `<Impl>::<method>` with no fn-definition
//! node behind it. The touchpoint walker's phantom gate rejects such
//! sinks, which would leave the adapter looking non-delegating even
//! though the trait anchor IS a valid target capability. This pass
//! runs after every fn-definition has been added as a node, so
//! "real node" can be decided by `forward.contains_key`, and rewrites
//! each phantom edge to point at `<Trait>::<method>` instead.

use super::super::type_infer::WorkspaceTypeIndex;
use super::CallGraph;
use std::collections::HashSet;

/// Rewrite phantom inherited-default edges to their trait anchors.
pub(super) fn rewrite_phantom_inherited_default_edges(
    graph: &mut CallGraph,
    type_index: &WorkspaceTypeIndex,
) {
    let real_nodes: HashSet<String> = graph.forward.keys().cloned().collect();
    let rewrites = collect_phantom_rewrites(graph, &real_nodes, type_index);
    for (caller, old_callee, new_callee) in rewrites {
        apply_edge_rewrite(graph, &caller, &old_callee, new_callee);
    }
}

/// Per-edge probe: callee is phantom + matches an inherited-default
/// impl ⇒ produce `(caller, old_callee, new_anchor)` triple.
fn collect_phantom_rewrites(
    graph: &CallGraph,
    real_nodes: &HashSet<String>,
    type_index: &WorkspaceTypeIndex,
) -> Vec<(String, String, String)> {
    let mut rewrites: Vec<(String, String, String)> = Vec::new();
    for (caller, callees) in &graph.forward {
        for callee in callees {
            if real_nodes.contains(callee) {
                continue;
            }
            if let Some(anchor) = inherited_default_anchor_for(callee, type_index) {
                rewrites.push((caller.clone(), callee.clone(), anchor));
            }
        }
    }
    rewrites
}

/// Apply one edge rewrite to both forward and reverse maps. Shared
/// primitive — the per-rewrite probes (`collect_phantom_rewrites`,
/// `collect_reexport_rewrites`) differ in policy but the actual map
/// mutation is identical, so it lives here as a single chokepoint.
pub(crate) fn apply_edge_rewrite(graph: &mut CallGraph, caller: &str, old: &str, new: String) {
    if let Some(set) = graph.forward.get_mut(caller) {
        set.remove(old);
        set.insert(new.clone());
    }
    if let Some(set) = graph.reverse.get_mut(old) {
        set.remove(caller);
    }
    graph
        .reverse
        .entry(new)
        .or_default()
        .insert(caller.to_string());
}

/// True iff `canonical` matches an `<Impl>::<method>` whose `Impl`
/// inherits the default body of EXACTLY ONE trait method. Returns
/// the trait anchor canonical when the match is unambiguous;
/// `None` otherwise (no candidate, or multiple traits with the same
/// default method name — Rust would require UFCS disambiguation in
/// that case, and we can't tell from the canonical alone which trait
/// was selected, so we leave the canonical phantom rather than guess
/// from HashMap iteration order).
fn inherited_default_anchor_for(
    canonical: &str,
    type_index: &WorkspaceTypeIndex,
) -> Option<String> {
    let (impl_canon, method) = canonical.rsplit_once("::")?;
    let mut candidate: Option<String> = None;
    for (trait_canon, impls) in &type_index.trait_impls {
        if !is_inherited_default_match(type_index, trait_canon, impls, impl_canon, method) {
            continue;
        }
        if candidate.is_some() {
            return None;
        }
        candidate = Some(format!("{trait_canon}::{method}"));
    }
    candidate
}

/// Predicate: trait declares `method` with a default body, has
/// `impl_canon` in its impl list, and the impl does NOT override the
/// method. Extracted so `inherited_default_anchor_for` stays under
/// the SRP cyclomatic budget after adding the ambiguity guard.
fn is_inherited_default_match(
    type_index: &WorkspaceTypeIndex,
    trait_canon: &str,
    impls: &[String],
    impl_canon: &str,
    method: &str,
) -> bool {
    if !impls.iter().any(|i| i == impl_canon) {
        return false;
    }
    if !type_index
        .trait_methods
        .get(trait_canon)
        .is_some_and(|s| s.contains(method))
    {
        return false;
    }
    if !type_index.trait_method_has_default_body(trait_canon, method) {
        return false;
    }
    !type_index.impl_overrides_method(trait_canon, impl_canon, method)
}
