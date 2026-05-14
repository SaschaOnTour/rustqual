//! Per-adapter coverage aggregation + adapter-reachable-targets BFS
//! for Check B.
//!
//! `AdapterCoverage` aggregates each adapter's per-handler touchpoint
//! sets into one union per adapter — Check B's orphan probe needs the
//! per-adapter-level set, not the per-handler grain that
//! `HandlerTouchpoints` carries. The reachable-targets BFS expands
//! the union forward through target-internal callers so post-boundary
//! plumbing wired into adapter coverage stays silent (only genuine
//! orphans / dead islands surface).

use super::anchor_index::is_anchor_target_capability;
use super::pub_fns::PubFnInfo;
use super::workspace_graph::{canonical_name_for_pub_fn, CallGraph};
use super::HandlerTouchpoints;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use std::collections::{HashMap, HashSet, VecDeque};

/// Per-adapter aggregated touchpoint set: union of every adapter
/// pub-fn's individual touchpoint set, keyed by adapter layer name.
pub(super) type AdapterCoverage = HashMap<String, HashSet<String>>;

/// Build the per-adapter coverage view by unioning the cached
/// touchpoint sets across each adapter's handlers. Deprecated
/// handlers are already filtered out of `touchpoints`.
/// Operation: nested fold over the cache.
pub(super) fn build_adapter_coverage(
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'_>>>,
    touchpoints: &HandlerTouchpoints,
    cp: &CompiledCallParity,
) -> AdapterCoverage {
    let mut coverage: AdapterCoverage = HashMap::new();
    for adapter in &cp.adapters {
        let mut union: HashSet<String> = HashSet::new();
        if let Some(handlers) = pub_fns_by_layer.get(adapter) {
            for info in handlers {
                let canonical = canonical_name_for_pub_fn(info);
                if let Some(tps) = touchpoints.get(&canonical) {
                    union.extend(tps.iter().cloned());
                }
            }
        }
        coverage.insert(adapter.clone(), union);
    }
    coverage
}

/// Target-capability nodes transitively reachable from any adapter
/// touchpoint. Distinguishes post-boundary plumbing (silent) from
/// genuine orphans (flagged). Forward BFS over `graph.forward`,
/// gated by `is_target_capability_node` so trait anchors reached
/// via target-internal callers stay visible.
pub(super) fn build_adapter_reachable_targets(
    coverage: &AdapterCoverage,
    graph: &CallGraph,
    target_layer: &str,
    adapter_layers: &[String],
) -> HashSet<String> {
    let mut reachable: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    for tps in coverage.values() {
        for tp in tps {
            if reachable.insert(tp.clone()) {
                queue.push_back(tp.clone());
            }
        }
    }
    let probe = ReachProbe {
        graph,
        target_layer,
        adapter_layers,
    };
    while let Some(node) = queue.pop_front() {
        propagate_anchor_impls(&probe, &node, &mut reachable, &mut queue);
        propagate_callees(&probe, &node, &mut reachable, &mut queue);
    }
    reachable
}

/// Read-only inputs bundled to keep BFS-step helpers under the
/// SRP-parameter budget. Operation: data container.
struct ReachProbe<'a> {
    graph: &'a CallGraph,
    target_layer: &'a str,
    adapter_layers: &'a [String],
}

/// Trait-method anchors are synthetic — no fn body to walk, but
/// `AnchorInfo` lists every impl method that satisfies the anchor.
/// A reaching adapter dispatches into those impls at runtime via the
/// trait, so they (and their own target-internal callees) must
/// propagate too. Without this step, generic-fn dispatch chains like
/// `record_query::<Q> → Q::execute → impl_method → helper` would
/// surface `helper` as a false orphan even though the adapter reaches
/// it via the impl. Operation: per-impl enqueue.
fn propagate_anchor_impls(
    probe: &ReachProbe<'_>,
    node: &str,
    reachable: &mut HashSet<String>,
    queue: &mut VecDeque<String>,
) {
    let Some(info) = probe.graph.trait_method_anchors.get(node) else {
        return;
    };
    for impl_canonical in &info.impl_method_canonicals {
        if is_target_capability_node(
            impl_canonical,
            probe.graph,
            probe.target_layer,
            probe.adapter_layers,
        ) && reachable.insert(impl_canonical.clone())
        {
            queue.push_back(impl_canonical.clone());
        }
    }
}

/// Forward-edge fan-out: enqueue every target-capability callee not
/// already in the reachable set. Operation: per-callee enqueue.
fn propagate_callees(
    probe: &ReachProbe<'_>,
    node: &str,
    reachable: &mut HashSet<String>,
    queue: &mut VecDeque<String>,
) {
    let Some(callees) = probe.graph.forward.get(node) else {
        return;
    };
    for callee in callees {
        if is_target_capability_node(
            callee,
            probe.graph,
            probe.target_layer,
            probe.adapter_layers,
        ) && reachable.insert(callee.clone())
        {
            queue.push_back(callee.clone());
        }
    }
}

/// True when `canonical` is a real target-layer graph node or a
/// trait-method anchor passing the unified target-capability rule.
/// Phantom edge sinks (e.g. fabricated `<Impl>::<method>` canonicals
/// from inherited-default impls) carry a cached `layer_of` but no
/// `forward` entry; they must not propagate as target capabilities.
fn is_target_capability_node(
    canonical: &str,
    graph: &CallGraph,
    target_layer: &str,
    adapter_layers: &[String],
) -> bool {
    if graph.layer_of(canonical) == Some(target_layer) && graph.forward.contains_key(canonical) {
        return true;
    }
    graph
        .trait_method_anchors
        .get(canonical)
        .is_some_and(|info| is_anchor_target_capability(info, target_layer, adapter_layers))
}
