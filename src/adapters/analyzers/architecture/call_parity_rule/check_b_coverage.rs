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

/// Set of target-layer canonicals transitively reachable from at least
/// one adapter touchpoint, traversing only target-capability edges.
///
/// Used to distinguish post-boundary helpers (wired into adapter
/// coverage via target-internal callers — silent) from genuine
/// orphans and dead target-layer islands (flagged). Multi-source
/// forward BFS seeded from the touchpoint union.
///
/// A callee counts as a target-capability node when EITHER (a) its
/// resolved layer matches `target_layer`, OR (b) it is a synthetic
/// trait-method anchor that passes the unified
/// `is_anchor_target_capability` rule for `(target_layer, adapter_layers)`.
/// Without (b), a `dyn Trait.method()` dispatch reached transitively
/// from an adapter would be invisible to the BFS (anchor's
/// `layer_of()` is the trait declaration layer, e.g. `ports`), and
/// Check B would falsely flag the anchor as orphan even though an
/// adapter wires it up via a target-internal caller.
/// Operation: BFS over `graph.forward`, gated by the unified
/// target-capability predicate.
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
    while let Some(node) = queue.pop_front() {
        let Some(callees) = graph.forward.get(&node) else {
            continue;
        };
        for callee in callees {
            if is_target_capability_node(callee, graph, target_layer, adapter_layers)
                && reachable.insert(callee.clone())
            {
                queue.push_back(callee.clone());
            }
        }
    }
    reachable
}

/// True when `canonical` is either a direct target-layer node or a
/// synthetic trait-method anchor that the unified rule promotes to a
/// target capability. Operation: predicate composition.
fn is_target_capability_node(
    canonical: &str,
    graph: &CallGraph,
    target_layer: &str,
    adapter_layers: &[String],
) -> bool {
    if graph.layer_of(canonical) == Some(target_layer) {
        return true;
    }
    graph
        .trait_method_anchors
        .get(canonical)
        .is_some_and(|info| is_anchor_target_capability(info, target_layer, adapter_layers))
}
