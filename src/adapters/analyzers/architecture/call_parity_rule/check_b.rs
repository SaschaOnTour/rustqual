//! Check B — Parity-Coverage (boundary semantic).
//!
//! For each `pub fn` in the configured target layer, count how many
//! adapters touch it directly at the **boundary** (first crossing into
//! the target layer from each adapter pub-fn). Compare those reach sets
//! across adapters. A target T is flagged when:
//!
//! - Some adapter touches T at the boundary AND another adapter doesn't
//!   (mismatch case — feature-coverage drift), OR
//! - No adapter touches T at the boundary AND T has no callers within
//!   the target layer either (orphan case — application capability
//!   that isn't wired to any adapter).
//!
//! The intermediate case — T isn't touched by any adapter directly but
//! IS called by other application fns (post-boundary plumbing like
//! `record_operation`, `impact_count`) — is silent. That used to fire
//! under v1.2.0's leaf-reachability semantic; v1.2.1 deliberately
//! drops it. Internal application chains aren't a parity concern.
//!
//! Two escape mechanisms:
//! - `exclude_targets` glob in the call-parity config (matched against
//!   the canonical minus `crate::` prefix).
//! - `// qual:allow(architecture)` above the target fn — handled by the
//!   architecture-dimension suppression pipeline.

use super::pub_fns::PubFnInfo;
use super::workspace_graph::{canonical_name_for_pub_fn, CallGraph};
use super::HandlerTouchpoints;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::{HashMap, HashSet};

// qual:api
/// Emit one `CallParityMissingAdapter` finding per target pub-fn whose
/// boundary-reach set isn't symmetric across the configured adapters.
/// Integration: builds the per-adapter coverage view from the shared
/// `HandlerTouchpoints` cache, then per-target finding construction
/// via `inspect_target`.
pub(crate) fn check_missing_adapter<'ast>(
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'ast>>>,
    graph: &CallGraph,
    touchpoints: &HandlerTouchpoints,
    cp: &CompiledCallParity,
) -> Vec<MatchLocation> {
    let Some(targets) = pub_fns_by_layer.get(&cp.target) else {
        return Vec::new();
    };
    let coverage = build_adapter_coverage(pub_fns_by_layer, touchpoints, cp);
    let ctx = TargetCtx {
        graph,
        cp,
        coverage: &coverage,
    };
    let mut out = Vec::new();
    for info in targets {
        if let Some(hit) = inspect_target(info, &ctx) {
            out.push(hit);
        }
    }
    out
}

/// Per-adapter aggregated touchpoint set: union of every adapter
/// pub-fn's individual touchpoint set, keyed by adapter layer name.
type AdapterCoverage = HashMap<String, HashSet<String>>;

/// Build the per-adapter coverage view by unioning the cached
/// touchpoint sets across each adapter's handlers. Deprecated
/// handlers are already filtered out of `touchpoints`.
/// Operation: nested fold over the cache.
fn build_adapter_coverage(
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

/// Read-only context bundle threaded into `inspect_target`. Operation:
/// data container.
struct TargetCtx<'a> {
    graph: &'a CallGraph,
    cp: &'a CompiledCallParity,
    coverage: &'a AdapterCoverage,
}

/// Decide whether one target pub-fn produces a finding under the
/// boundary semantic. Returns `Some(hit)` for mismatch or orphan,
/// `None` otherwise (silent for excluded targets and post-boundary
/// helpers).
/// Integration: dispatches on coverage shape via `classify_target`.
fn inspect_target(info: &PubFnInfo<'_>, ctx: &TargetCtx<'_>) -> Option<MatchLocation> {
    let canonical = canonical_name_for_pub_fn(info);
    if is_excluded(&canonical, ctx.cp) {
        return None;
    }
    let reached = adapters_reaching(&canonical, ctx.coverage, &ctx.cp.adapters);
    let missing = adapters_missing(&reached, &ctx.cp.adapters);
    if missing.is_empty() {
        return None;
    }
    if reached.is_empty() && has_target_layer_caller(&canonical, ctx.graph, &ctx.cp.target) {
        return None;
    }
    Some(build_finding(
        info,
        canonical,
        reached,
        missing,
        &ctx.cp.target,
    ))
}

/// True iff the canonical target matches an `exclude_targets` glob.
/// Operation: prefix strip + globset probe.
fn is_excluded(canonical: &str, cp: &CompiledCallParity) -> bool {
    let stripped = canonical.strip_prefix("crate::").unwrap_or(canonical);
    cp.exclude_targets.is_match(stripped)
}

/// Adapters whose coverage set contains `target`. Operation: filter.
fn adapters_reaching(target: &str, coverage: &AdapterCoverage, adapters: &[String]) -> Vec<String> {
    adapters
        .iter()
        .filter(|a| {
            coverage
                .get(a.as_str())
                .is_some_and(|set| set.contains(target))
        })
        .cloned()
        .collect()
}

/// Adapters listed in config but NOT present in `reached`.
/// Operation: set difference.
fn adapters_missing(reached: &[String], adapters: &[String]) -> Vec<String> {
    adapters
        .iter()
        .filter(|a| !reached.iter().any(|r| r == a.as_str()))
        .cloned()
        .collect()
}

/// True iff `target` has at least one caller in the target layer
/// itself — distinguishes post-boundary helpers (silent) from genuine
/// orphans (flagged).
/// Operation: lookup + layer probe.
fn has_target_layer_caller(target: &str, graph: &CallGraph, target_layer: &str) -> bool {
    let Some(callers) = graph.reverse.get(target) else {
        return false;
    };
    callers
        .iter()
        .any(|c| graph.layer_of(c) == Some(target_layer))
}

/// Construct a `CallParityMissingAdapter` MatchLocation. Operation:
/// data construction + sort.
fn build_finding(
    info: &PubFnInfo<'_>,
    canonical: String,
    mut reached: Vec<String>,
    missing: Vec<String>,
    target_layer: &str,
) -> MatchLocation {
    reached.sort();
    MatchLocation {
        file: info.file.clone(),
        line: info.line,
        column: 0,
        kind: ViolationKind::CallParityMissingAdapter {
            target_fn: canonical,
            target_layer: target_layer.to_string(),
            reached_adapters: reached,
            missing_adapters: missing,
        },
    }
}
