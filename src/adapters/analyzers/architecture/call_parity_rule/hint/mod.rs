//! Discoverable hints for `CallParityMissingAdapter` findings —
//! point the author at private + non-stdlib-attributed fns in the
//! missing adapter that would resolve the finding if their attribute
//! were promoted via `[architecture.call_parity] promoted_attributes`.
//! See `candidates::collect_private_candidates` for the candidate-
//! selection walk and `enrich_with_hints` for how candidates are
//! projected onto findings.
//!
//! Reachability check uses the SAME `compute_touchpoints` walker the
//! production check uses. No parallel reachability logic — any
//! `call_depth` / peer-adapter / boundary-stop change in the walker
//! flows through automatically. A candidate appears in the hint iff
//! `compute_touchpoints(candidate)` (treating the candidate as if it
//! were a handler) contains the unreached target — i.e. promoting
//! its attribute will ACTUALLY put the target in the adapter's
//! coverage set.

mod candidates;

pub(super) use candidates::{collect_private_candidates, PrivateCandidate};

use super::touchpoints::{compute_touchpoints, TouchpointContext};
use super::workspace_graph::CallGraph;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::{HashMap, HashSet};

/// Attach a hint to every `CallParityMissingAdapter` finding for
/// which a private attributed candidate would resolve the gap.
/// Empty `candidates` short-circuits — the common case when no
/// promotable attribute exists in the workspace.
pub(super) fn enrich_with_hints(
    findings: &mut [MatchLocation],
    graph: &CallGraph,
    cp: &CompiledCallParity,
    candidates: &[PrivateCandidate],
) {
    if candidates.is_empty() {
        return;
    }
    let by_adapter = group_by_adapter(candidates);
    let mut probe = HintProbe {
        graph,
        cp,
        by_adapter: &by_adapter,
        candidate_touchpoints: HashMap::new(),
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

/// Per-call probe state. `candidate_touchpoints` memoises one
/// `compute_touchpoints` walk per candidate so a candidate that's
/// relevant to multiple findings (or to multiple missing adapters of
/// the same finding) only pays one walk. The walk is delegated to
/// the production touchpoint code — by construction, a candidate is
/// listed in the hint iff promoting it would put the target in the
/// adapter's coverage set under the same call_depth + peer-adapter
/// + boundary rules Check B uses.
struct HintProbe<'a> {
    graph: &'a CallGraph,
    cp: &'a CompiledCallParity,
    by_adapter: &'a HashMap<&'a str, Vec<&'a PrivateCandidate>>,
    candidate_touchpoints: HashMap<String, HashSet<String>>,
}

impl<'a> HintProbe<'a> {
    fn hint_for(&mut self, target_fn: &str, missing_adapters: &[String]) -> Option<String> {
        let by_adapter = self.collect_hits_per_adapter(missing_adapters, target_fn);
        if by_adapter.is_empty() {
            None
        } else {
            Some(format_hint(&by_adapter))
        }
    }

    /// Per missing adapter, take its candidates and keep the ones
    /// whose `compute_touchpoints` (treating the candidate as a
    /// handler in that adapter) contains `target_fn`. Sort by
    /// (file, line, fn_name) for deterministic hint output.
    fn collect_hits_per_adapter(
        &mut self,
        missing_adapters: &[String],
        target_fn: &str,
    ) -> Vec<(String, Vec<&'a PrivateCandidate>)> {
        let mut out: Vec<(String, Vec<&PrivateCandidate>)> = Vec::new();
        for adapter in missing_adapters {
            let Some(adapter_candidates) = self.by_adapter.get(adapter.as_str()) else {
                continue;
            };
            let mut hits: Vec<&PrivateCandidate> = Vec::new();
            for candidate in adapter_candidates.iter().copied() {
                if self.candidate_reaches_target(candidate, adapter, target_fn) {
                    hits.push(candidate);
                }
            }
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

    fn candidate_reaches_target(
        &mut self,
        candidate: &PrivateCandidate,
        adapter: &str,
        target_fn: &str,
    ) -> bool {
        let graph = self.graph;
        let cp = self.cp;
        let canonical = candidate.canonical.clone();
        let touchpoints = self
            .candidate_touchpoints
            .entry(canonical.clone())
            .or_insert_with(|| {
                let ctx = TouchpointContext {
                    graph,
                    target_layer: &cp.target,
                    call_depth: cp.call_depth,
                    origin_adapter: adapter,
                    adapter_layers: &cp.adapters,
                };
                compute_touchpoints(&canonical, &ctx)
            });
        touchpoints.contains(target_fn)
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
