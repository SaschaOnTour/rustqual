//! Check B — Parity-Coverage.
//!
//! Every `pub fn` in the configured target layer must be (transitively,
//! up to `call_depth` hops) reached from every adapter layer. A target
//! fn that's only used by some adapters signals asymmetric feature
//! coverage — the MCP handler delegates to it, but the REST handler
//! doesn't, or vice versa.
//!
//! The check walks the pre-built `CallGraph` **backward** from each
//! target pub-fn, collects the files of the callers, maps those to
//! layer names, and complains if the configured adapter set isn't
//! fully covered.
//!
//! Two escape mechanisms:
//! - `exclude_targets` glob in the call-parity config (matched against
//!   the canonical minus `crate::` prefix). Legitimate asymmetric
//!   target fns — `setup`, debug-only endpoints — live here.
//! - `// qual:allow(architecture)` above the target fn. Handled by the
//!   existing architecture-dimension suppression pipeline, so this
//!   check doesn't need its own filter.

use super::pub_fns::PubFnInfo;
use super::workspace_graph::{canonical_name_for_pub_fn, CallGraph, WalkState};
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::{HashMap, HashSet};

// qual:api
/// Emit one `CallParityMissingAdapter` finding per target pub-fn whose
/// transitive caller set doesn't cover every configured adapter layer.
/// Integration: per-target-fn reachability + missing-set construction.
pub(crate) fn check_missing_adapter<'ast>(
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'ast>>>,
    graph: &CallGraph,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
) -> Vec<MatchLocation> {
    let Some(targets) = pub_fns_by_layer.get(&cp.target) else {
        return Vec::new();
    };
    let adapter_set: HashSet<String> = cp.adapters.iter().cloned().collect();
    let mut out = Vec::new();
    for info in targets {
        let canonical = canonical_name_for_pub_fn(info);
        if is_excluded(&canonical, cp) {
            continue;
        }
        let reached =
            reached_adapter_layers(&canonical, graph, layers, &adapter_set, cp.call_depth);
        let missing: Vec<String> = cp
            .adapters
            .iter()
            .filter(|a| !reached.contains(a.as_str()))
            .cloned()
            .collect();
        if missing.is_empty() {
            continue;
        }
        let mut reached_sorted: Vec<String> = reached.into_iter().collect();
        reached_sorted.sort();
        out.push(MatchLocation {
            file: info.file.clone(),
            line: info.line,
            column: 0,
            kind: ViolationKind::CallParityMissingAdapter {
                target_fn: canonical,
                target_layer: cp.target.clone(),
                reached_adapters: reached_sorted,
                missing_adapters: missing,
            },
        });
    }
    out
}

/// True iff the canonical target matches an `exclude_targets` glob.
/// Glob strings are matched against the canonical minus the leading
/// `crate::` — stripping it once here keeps user config readable
/// (`application::setup::*` rather than `crate::application::setup::*`).
/// Operation: prefix strip + globset probe.
fn is_excluded(canonical: &str, cp: &CompiledCallParity) -> bool {
    let stripped = canonical.strip_prefix("crate::").unwrap_or(canonical);
    cp.exclude_targets.is_match(stripped)
}

/// BFS backward from `target` over `graph.reverse`; map each visited
/// caller-node to its file's layer and return the intersection with
/// `adapter_set`.
/// Integration: seeds + delegates step evaluation.
fn reached_adapter_layers(
    target: &str,
    graph: &CallGraph,
    layers: &LayerDefinitions,
    adapter_set: &HashSet<String>,
    call_depth: usize,
) -> HashSet<String> {
    CoverageWalk {
        graph,
        layers,
        adapter_set,
        call_depth,
    }
    .run(target)
}

struct CoverageWalk<'a> {
    graph: &'a CallGraph,
    layers: &'a LayerDefinitions,
    adapter_set: &'a HashSet<String>,
    call_depth: usize,
}

impl CoverageWalk<'_> {
    fn run(&self, target: &str) -> HashSet<String> {
        let mut reached = HashSet::new();
        let Some(direct) = self.graph.reverse.get(target) else {
            return reached;
        };
        let mut state = WalkState::seeded(target, direct);
        while let Some((node, depth)) = state.queue.pop_front() {
            if !state.visited.insert(node.clone()) {
                continue;
            }
            self.record_node_layer(&node, &mut reached);
            if depth < self.call_depth {
                if let Some(callers) = self.graph.reverse.get(&node) {
                    state.enqueue_unvisited(callers, depth + 1);
                }
            }
        }
        reached
    }

    fn record_node_layer(&self, node: &str, reached: &mut HashSet<String>) {
        let Some(file) = self.graph.node_file.get(node) else {
            return;
        };
        let Some(layer) = self.layers.layer_for_file(file) else {
            return;
        };
        if self.adapter_set.contains(layer) {
            reached.insert(layer.to_string());
        }
    }
}
