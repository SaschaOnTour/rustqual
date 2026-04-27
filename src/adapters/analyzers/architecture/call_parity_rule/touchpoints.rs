//! Boundary-only touchpoint computation.
//!
//! Given a handler's canonical name and the workspace call graph, walk
//! forward (BFS) through adapter-internal helpers up to `call_depth`
//! hops. Once a node in the **target layer** is reached, record it as a
//! touchpoint and **do not** descend into its callees. The walk
//! terminates either on depth exhaustion or when no unvisited neighbors
//! remain.
//!
//! This is the shared core for the four call-parity checks:
//!
//! - Check A fails when the returned set is empty (no delegation).
//! - Check B fails when a touchpoint is in some adapter's coverage but
//!   missing from another's.
//! - Check C fails when a single handler's touchpoint set has size > 1.
//! - Check D fails when per-touchpoint counts diverge across adapters.
//!
//! The boundary stop is the semantic distinguisher from leaf-reachability:
//! application-internal call chains (e.g. `session.search` →
//! `record_operation` → `impact_count`) are NOT inspected. Only the
//! first crossing into the target layer counts.

use super::workspace_graph::{CallGraph, WalkState};
use std::collections::HashSet;

// qual:api
/// Compute the set of target-layer canonical names reached from `handler`
/// by a forward BFS that stops on first target-layer entry per path.
///
/// `call_depth` bounds the number of adapter-internal hops the walk
/// will traverse. Hops within the target layer are not traversed at
/// all — once a target-layer node is hit, it joins the touchpoint set
/// and the walk does not enqueue its callees.
///
/// Integration: seeds the BFS scaffold and delegates to `TouchpointWalk::run`.
pub(crate) fn compute_touchpoints(
    handler: &str,
    graph: &CallGraph,
    target_layer: &str,
    call_depth: usize,
) -> HashSet<String> {
    TouchpointWalk {
        graph,
        target_layer,
        call_depth,
    }
    .run(handler)
}

/// Read-only BFS driver: bundles static inputs so the step logic
/// stays off `compute_touchpoints`'s IOSP budget.
struct TouchpointWalk<'a> {
    graph: &'a CallGraph,
    target_layer: &'a str,
    call_depth: usize,
}

impl TouchpointWalk<'_> {
    /// BFS forward from `start`. Returns the set of target-layer
    /// canonicals encountered; does not traverse past the boundary.
    fn run(&self, start: &str) -> HashSet<String> {
        let mut touchpoints = HashSet::new();
        let Some(direct) = self.graph.forward.get(start) else {
            return touchpoints;
        };
        let mut state = WalkState::seeded(start, direct);
        while let Some((node, depth)) = state.queue.pop_front() {
            if self.graph.layer_of(&node) == Some(self.target_layer) {
                touchpoints.insert(node);
                continue;
            }
            if depth < self.call_depth {
                if let Some(callees) = self.graph.forward.get(&node) {
                    state.enqueue_unvisited(callees, depth + 1);
                }
            }
        }
        touchpoints
    }
}
