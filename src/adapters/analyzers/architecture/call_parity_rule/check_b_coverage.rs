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
/// one adapter touchpoint, traversing only target-layer edges.
///
/// Used to distinguish post-boundary helpers (wired into adapter
/// coverage via target-internal callers — silent) from genuine
/// orphans and dead target-layer islands (flagged). Multi-source
/// forward BFS seeded from the touchpoint union.
/// Operation: BFS over `graph.forward`, gated by target layer.
pub(super) fn build_adapter_reachable_targets(
    coverage: &AdapterCoverage,
    graph: &CallGraph,
    target_layer: &str,
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
            if graph.layer_of(callee) == Some(target_layer) && reachable.insert(callee.clone()) {
                queue.push_back(callee.clone());
            }
        }
    }
    reachable
}
