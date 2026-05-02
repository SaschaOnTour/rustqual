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

/// Static inputs for one touchpoint walk. Bundling them keeps
/// `compute_touchpoints` at a manageable parameter count while making
/// the adapter context explicit at every call site.
pub(crate) struct TouchpointContext<'a> {
    pub graph: &'a CallGraph,
    pub target_layer: &'a str,
    pub call_depth: usize,
    /// Layer of the handler the walk starts from.
    pub origin_adapter: &'a str,
    /// All adapter-layer names from config. Layers in this list other
    /// than `origin_adapter` are peers and the walk refuses to descend
    /// into them (otherwise a `cli` handler that calls into an `mcp`
    /// handler would inherit MCP's touchpoints and mask Check A/B/D
    /// even though CLI never crossed into the target layer itself).
    pub adapter_layers: &'a [String],
}

// qual:api
/// Compute the set of target-layer canonical names reached from `handler`
/// by a forward BFS that stops on first target-layer entry per path
/// and refuses to descend into peer adapter layers.
///
/// Integration: seeds the BFS scaffold and delegates to `TouchpointWalk::run`.
pub(crate) fn compute_touchpoints(handler: &str, ctx: &TouchpointContext<'_>) -> HashSet<String> {
    TouchpointWalk { ctx }.run(handler)
}

/// Read-only BFS driver: bundles static inputs so the step logic
/// stays off `compute_touchpoints`'s IOSP budget.
struct TouchpointWalk<'a> {
    ctx: &'a TouchpointContext<'a>,
}

impl TouchpointWalk<'_> {
    /// BFS forward from `start`. Returns the set of target-layer
    /// canonicals encountered; does not traverse past the boundary
    /// or into peer adapter layers.
    fn run(&self, start: &str) -> HashSet<String> {
        let mut touchpoints = HashSet::new();
        let Some(direct) = self.ctx.graph.forward.get(start) else {
            return touchpoints;
        };
        let mut state = WalkState::seeded(start, direct);
        while let Some((node, depth)) = state.queue.pop_front() {
            if self.ctx.graph.layer_of(&node) == Some(self.ctx.target_layer) {
                touchpoints.insert(node);
                continue;
            }
            if self.is_peer_adapter(&node) {
                continue;
            }
            if depth < self.ctx.call_depth {
                if let Some(callees) = self.ctx.graph.forward.get(&node) {
                    state.enqueue_unvisited(callees, depth + 1);
                }
            }
        }
        touchpoints
    }

    /// True if `node` lives in an adapter layer that is not the origin
    /// adapter — those are peers and must not be traversed, otherwise
    /// the walk would inherit a peer's touchpoints.
    fn is_peer_adapter(&self, node: &str) -> bool {
        let Some(layer) = self.ctx.graph.layer_of(node) else {
            return false;
        };
        if layer == self.ctx.origin_adapter {
            return false;
        }
        self.ctx.adapter_layers.iter().any(|a| a == layer)
    }
}
