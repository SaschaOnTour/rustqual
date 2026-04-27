//! Check D — multiplicity mismatch.
//!
//! For each target pub-fn T that's in EVERY adapter's coverage (so
//! Check B is silent), compare the per-adapter handler counts for T.
//! If counts diverge — e.g. cli has 2 handlers reaching `session.search`
//! and mcp has 1 — emit a finding.
//!
//! Counts are over the **set** of handler canonical names whose
//! touchpoint set contains T (de-duplicated). A handler that calls T
//! multiple times in its body still counts as 1.
//!
//! Rationale: this catches the "alias accumulation" drift pattern —
//! cli grows backwards-compat aliases (`cmd_grep` for `cmd_search`)
//! while mcp doesn't, and the API surfaces silently diverge in
//! count even though both adapters technically cover the capability.

use super::pub_fns::PubFnInfo;
use super::workspace_graph::canonical_name_for_pub_fn;
use super::HandlerTouchpoints;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::HashMap;

// qual:api
/// Emit one `CallParityMultiplicityMismatch` finding per target pub-fn
/// whose handler counts differ across adapters.
/// Integration: builds per-adapter per-target counts from the shared
/// `HandlerTouchpoints` cache, then probes each target for divergence.
pub(crate) fn check_multiplicity_mismatch<'ast>(
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'ast>>>,
    touchpoints: &HandlerTouchpoints,
    cp: &CompiledCallParity,
) -> Vec<MatchLocation> {
    let Some(targets) = pub_fns_by_layer.get(&cp.target) else {
        return Vec::new();
    };
    let counts = build_per_adapter_target_counts(pub_fns_by_layer, touchpoints, cp);
    let mut out = Vec::new();
    for info in targets {
        if let Some(hit) = inspect_target(info, &counts, cp) {
            out.push(hit);
        }
    }
    out
}

/// Per-adapter, per-target handler count: `counts[adapter][target] = N`
/// where N is the number of distinct adapter pub-fns whose touchpoint
/// set contains `target`.
type AdapterTargetCounts = HashMap<String, HashMap<String, usize>>;

/// Accumulate per-(adapter, target) handler counts from the shared
/// `HandlerTouchpoints` cache. A handler is counted once per target
/// it touches; deprecated handlers are absent from the cache.
/// Integration: per-adapter counter rollup via `count_for_adapter`.
fn build_per_adapter_target_counts(
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'_>>>,
    touchpoints: &HandlerTouchpoints,
    cp: &CompiledCallParity,
) -> AdapterTargetCounts {
    let mut counts: AdapterTargetCounts = HashMap::new();
    for adapter in &cp.adapters {
        let per_target = count_for_adapter(pub_fns_by_layer.get(adapter), touchpoints);
        counts.insert(adapter.clone(), per_target);
    }
    counts
}

/// Per-target handler count for one adapter's handler list. Each
/// handler contributes one count per target it touches.
/// Operation: bump-counter loop.
fn count_for_adapter(
    handlers: Option<&Vec<PubFnInfo<'_>>>,
    touchpoints: &HandlerTouchpoints,
) -> HashMap<String, usize> {
    let mut per_target: HashMap<String, usize> = HashMap::new();
    let Some(handlers) = handlers else {
        return per_target;
    };
    for info in handlers {
        let canonical = canonical_name_for_pub_fn(info);
        let Some(tps) = touchpoints.get(&canonical) else {
            continue;
        };
        for tp in tps {
            *per_target.entry(tp.clone()).or_insert(0) += 1;
        }
    }
    per_target
}

/// Decide whether one target pub-fn has divergent counts. Returns
/// `Some(hit)` only when target appears in every adapter's count map
/// (otherwise it's a Check B concern) AND the count values differ.
/// Operation: per-target probe.
fn inspect_target(
    info: &PubFnInfo<'_>,
    counts: &AdapterTargetCounts,
    cp: &CompiledCallParity,
) -> Option<MatchLocation> {
    let canonical = canonical_name_for_pub_fn(info);
    let per_adapter = collect_counts(&canonical, counts, cp);
    if per_adapter.len() != cp.adapters.len() {
        return None;
    }
    if !counts_diverge(&per_adapter) {
        return None;
    }
    Some(build_finding(info, canonical, per_adapter, &cp.target))
}

/// Build the per-adapter count list for one target. Returns adapters
/// sorted by name; entries omitted when adapter doesn't reach target.
/// Operation: filter + sort.
fn collect_counts(
    target: &str,
    counts: &AdapterTargetCounts,
    cp: &CompiledCallParity,
) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = cp
        .adapters
        .iter()
        .filter_map(|a| {
            counts
                .get(a)
                .and_then(|m| m.get(target))
                .map(|c| (a.clone(), *c))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// True iff the count list contains at least two distinct values.
/// Operation: window probe.
fn counts_diverge(per_adapter: &[(String, usize)]) -> bool {
    per_adapter.windows(2).any(|w| w[0].1 != w[1].1)
}

/// Construct a `CallParityMultiplicityMismatch` MatchLocation.
/// Operation: data construction.
fn build_finding(
    info: &PubFnInfo<'_>,
    canonical: String,
    counts_per_adapter: Vec<(String, usize)>,
    target_layer: &str,
) -> MatchLocation {
    MatchLocation {
        file: info.file.clone(),
        line: info.line,
        column: 0,
        kind: ViolationKind::CallParityMultiplicityMismatch {
            target_fn: canonical,
            target_layer: target_layer.to_string(),
            counts_per_adapter,
        },
    }
}
