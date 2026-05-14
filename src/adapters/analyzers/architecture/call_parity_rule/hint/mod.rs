//! Discoverable hints for `CallParityMissingAdapter` findings —
//! point the author at private + non-stdlib-attributed fns in the
//! missing adapter that would resolve the finding if their attribute
//! were promoted via `[architecture.call_parity] promoted_attributes`.
//! See `candidates::collect_private_candidates` for the candidate-
//! selection walk and `enrich_with_hints` for how candidates are
//! projected onto findings via reverse-BFS reachability.
//!
//! Best-effort: the reachability probe runs on the workspace call
//! graph without applying `call_depth` or peer-adapter constraints
//! that the touchpoint walker enforces. A hint can therefore suggest
//! a promotion that the actual walker wouldn't accept (candidate
//! beyond depth, behind a peer-adapter boundary, in a hidden module
//! chain). The author re-runs after promoting; if the finding stays,
//! the hint was a false lead — cheap to verify, expensive to make
//! perfect.

mod candidates;

pub(super) use candidates::{collect_private_candidates, PrivateCandidate};

use super::workspace_graph::CallGraph;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::{HashMap, HashSet, VecDeque};

// qual:api
/// Attach a hint to every `CallParityMissingAdapter` finding for
/// which a private attributed candidate would resolve the gap.
/// Empty `candidates` short-circuits — the common case when no
/// promotable attribute exists in the workspace.
pub(super) fn enrich_with_hints(
    findings: &mut [MatchLocation],
    graph: &CallGraph,
    candidates: &[PrivateCandidate],
) {
    if candidates.is_empty() {
        return;
    }
    let by_adapter = group_by_adapter(candidates);
    let mut probe = HintProbe {
        graph,
        by_adapter: &by_adapter,
        upstream_cache: HashMap::new(),
    };
    for f in findings {
        if let ViolationKind::CallParityMissingAdapter {
            target_fn,
            missing_adapters,
            hint,
            ..
        } = &mut f.kind
        {
            *hint = probe.hint_for(target_fn, missing_adapters);
        }
    }
}

/// Per-call probe state. The `upstream_cache` memoises one reverse
/// BFS per unique `target_fn` so multiple findings on the same
/// target share a single graph walk.
struct HintProbe<'a> {
    graph: &'a CallGraph,
    by_adapter: &'a HashMap<&'a str, Vec<&'a PrivateCandidate>>,
    upstream_cache: HashMap<String, HashSet<String>>,
}

impl<'a> HintProbe<'a> {
    fn hint_for(&mut self, target_fn: &str, missing_adapters: &[String]) -> Option<String> {
        let graph = self.graph;
        let upstream = self
            .upstream_cache
            .entry(target_fn.to_string())
            .or_insert_with(|| reverse_bfs(graph, target_fn));
        let by_adapter = collect_hits_per_adapter(self.by_adapter, missing_adapters, upstream);
        if by_adapter.is_empty() {
            None
        } else {
            Some(format_hint(&by_adapter))
        }
    }
}

/// Reverse BFS from `target` over `graph.reverse` — every node from
/// which `target` is transitively reachable. Excludes `target`
/// itself; candidates can never coincide with the target since they
/// come from the missing-adapter layer, not the target layer.
fn reverse_bfs(graph: &CallGraph, target: &str) -> HashSet<String> {
    let mut upstream: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    enqueue_callers(graph, target, &mut upstream, &mut queue);
    while let Some(node) = queue.pop_front() {
        enqueue_callers(graph, &node, &mut upstream, &mut queue);
    }
    upstream
}

fn enqueue_callers(
    graph: &CallGraph,
    node: &str,
    upstream: &mut HashSet<String>,
    queue: &mut VecDeque<String>,
) {
    let Some(callers) = graph.reverse.get(node) else {
        return;
    };
    for c in callers {
        if upstream.insert(c.clone()) {
            queue.push_back(c.clone());
        }
    }
}

/// Group candidates by their layer name (skipping layerless files).
/// One pass over the input; downstream lookups become O(1) per
/// (finding, missing_adapter) pair.
fn group_by_adapter(candidates: &[PrivateCandidate]) -> HashMap<&str, Vec<&PrivateCandidate>> {
    let mut out: HashMap<&str, Vec<&PrivateCandidate>> = HashMap::new();
    for c in candidates {
        if let Some(layer) = c.layer.as_deref() {
            out.entry(layer).or_default().push(c);
        }
    }
    out
}

/// Per missing adapter, intersect its candidates with `upstream` and
/// keep adapters that have at least one hit. Sorted (file, line,
/// fn_name) for deterministic hint output.
fn collect_hits_per_adapter<'a>(
    by_adapter: &'a HashMap<&'a str, Vec<&'a PrivateCandidate>>,
    missing_adapters: &[String],
    upstream: &HashSet<String>,
) -> Vec<(String, Vec<&'a PrivateCandidate>)> {
    let mut out: Vec<(String, Vec<&PrivateCandidate>)> = Vec::new();
    for adapter in missing_adapters {
        let Some(adapter_candidates) = by_adapter.get(adapter.as_str()) else {
            continue;
        };
        let mut hits: Vec<&PrivateCandidate> = adapter_candidates
            .iter()
            .copied()
            .filter(|c| upstream.contains(&c.canonical))
            .collect();
        if !hits.is_empty() {
            hits.sort_by(|a, b| {
                a.file
                    .cmp(&b.file)
                    .then(a.line.cmp(&b.line))
                    .then(a.fn_name.cmp(&b.fn_name))
            });
            out.push((adapter.clone(), hits));
        }
    }
    out
}

/// Render the final hint string. Operation: per-adapter block
/// assembly.
fn format_hint(by_adapter: &[(String, Vec<&PrivateCandidate>)]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for (adapter, hits) in by_adapter {
        let (noun, verb) = if hits.len() == 1 {
            ("fn", "reaches")
        } else {
            ("fns", "reach")
        };
        lines.push(format!(
            "{} private {noun} in {adapter} transitively {verb} this target:",
            hits.len()
        ));
        for c in hits {
            let attrs = c
                .attr_names
                .iter()
                .map(|n| format!("#[{n}]"))
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(format!(
                "  - {}:{} {} has {} attribute(s)",
                c.file, c.line, c.fn_name, attrs
            ));
        }
    }
    lines.push(
        "Add the attribute name to `[architecture.call_parity] promoted_attributes` if it marks a macro-generated handler entry point."
            .to_string(),
    );
    lines.join("\n")
}
