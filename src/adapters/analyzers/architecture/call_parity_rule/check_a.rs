//! Check A — Adapter-muss-delegieren.
//!
//! Every `pub fn` in a configured adapter layer must reach (directly or
//! transitively, up to `call_depth` hops) at least one fn in the
//! configured target layer. A fn that satisfies this delegates to the
//! shared Application layer; a fn that fails has almost certainly
//! inlined business logic.
//!
//! The check walks the pre-built `CallGraph` forward from each adapter
//! pub-fn, breadth-first with a depth cap and visited-set. `<method>:…`
//! and `<bare>:…` canonicals are layer-unknown by construction — they
//! can't count as a delegation target, which is the right conservative
//! stance (we can't prove the method resolves into the target layer).

use super::pub_fns::PubFnInfo;
use super::workspace_graph::{canonical_name_for_pub_fn, CallGraph};
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::{HashMap, HashSet, VecDeque};

// qual:api
/// Emit one `CallParityNoDelegation` finding per adapter pub-fn that
/// fails to reach the target layer within `call_depth` hops.
/// Integration: per-fn BFS + per-finding construction.
pub(crate) fn check_no_delegation<'ast>(
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'ast>>>,
    graph: &CallGraph,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
) -> Vec<MatchLocation> {
    let mut out = Vec::new();
    for adapter_layer in &cp.adapters {
        let Some(fns) = pub_fns_by_layer.get(adapter_layer) else {
            continue;
        };
        for info in fns {
            if fn_reaches_target(info, graph, layers, &cp.target, cp.call_depth) {
                continue;
            }
            out.push(MatchLocation {
                file: info.file.clone(),
                line: info.line,
                column: 0,
                kind: ViolationKind::CallParityNoDelegation {
                    fn_name: info.fn_name.clone(),
                    adapter_layer: adapter_layer.clone(),
                    target_layer: cp.target.clone(),
                    call_depth: cp.call_depth,
                },
            });
        }
    }
    out
}

/// True iff a BFS forward from `info`'s canonical name reaches any fn
/// living in the target layer within `call_depth` hops.
/// Integration: seeds + delegates to `TargetReachWalk::run`.
fn fn_reaches_target(
    info: &PubFnInfo<'_>,
    graph: &CallGraph,
    layers: &LayerDefinitions,
    target_layer: &str,
    call_depth: usize,
) -> bool {
    let start = canonical_name_for_pub_fn(info);
    TargetReachWalk {
        graph,
        layers,
        target_layer,
        call_depth,
    }
    .run(&start)
}

/// Read-only BFS driver: bundles the static inputs so the step logic
/// stays off `fn_reaches_target`'s IOSP budget.
struct TargetReachWalk<'a> {
    graph: &'a CallGraph,
    layers: &'a LayerDefinitions,
    target_layer: &'a str,
    call_depth: usize,
}

impl TargetReachWalk<'_> {
    /// BFS forward from `start`. Returns true on the first node that
    /// resolves to the target layer.
    fn run(&self, start: &str) -> bool {
        let Some(direct) = self.graph.forward.get(start) else {
            return false;
        };
        let mut state = WalkState::seeded(start, direct);
        while let Some((node, depth)) = state.queue.pop_front() {
            if !state.visited.insert(node.clone()) {
                continue;
            }
            if self.layers.layer_of_crate_path(&node) == Some(self.target_layer) {
                return true;
            }
            if depth < self.call_depth {
                if let Some(callees) = self.graph.forward.get(&node) {
                    state.enqueue_unvisited(callees, depth + 1);
                }
            }
        }
        false
    }
}

struct WalkState {
    queue: VecDeque<(String, usize)>,
    visited: HashSet<String>,
}

impl WalkState {
    fn seeded(start: &str, direct: &HashSet<String>) -> Self {
        let mut visited = HashSet::new();
        visited.insert(start.to_string());
        Self {
            queue: direct.iter().map(|c| (c.clone(), 1)).collect(),
            visited,
        }
    }

    fn enqueue_unvisited(&mut self, callees: &HashSet<String>, depth: usize) {
        for c in callees {
            if !self.visited.contains(c) {
                self.queue.push_back((c.clone(), depth));
            }
        }
    }
}
