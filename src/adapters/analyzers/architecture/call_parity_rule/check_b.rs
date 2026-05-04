//! Check B — Parity-Coverage (boundary semantic).
//!
//! For each `pub fn` in the configured target layer, count how many
//! adapters touch it directly at the **boundary** (first crossing into
//! the target layer from each adapter pub-fn). Compare those reach sets
//! across adapters. A target T is flagged when:
//!
//! - Some adapter touches T at the boundary AND another adapter doesn't
//!   (mismatch case — feature-coverage drift), OR
//! - T is **not transitively reachable** from any adapter touchpoint
//!   via target-internal callers (orphan case — application capability
//!   not wired to any adapter, including dead target-layer islands
//!   where T is only called by other unreachable target fns).
//!
//! The intermediate case — T isn't touched by any adapter directly but
//! IS reachable through some adapter via target-internal callers
//! (post-boundary plumbing like `record_operation`, `impact_count`
//! when an adapter reaches `session.search`) — is silent. That used to
//! fire under v1.2.0's leaf-reachability semantic; v1.2.1 deliberately
//! drops it. Internal application chains wired up via at least one
//! adapter aren't a parity concern.
//!
//! The reachability set is computed by `build_adapter_reachable_targets`
//! (multi-source forward BFS from the touchpoint union, traversing only
//! target-layer edges). A merely-existing target-layer caller no longer
//! suppresses the orphan branch — only a *live* one (transitively
//! reachable from some adapter) does.
//!
//! Two escape mechanisms:
//! - `exclude_targets` glob in the call-parity config (matched against
//!   the canonical minus `crate::` prefix).
//! - `// qual:allow(architecture)` above the target fn — handled by the
//!   architecture-dimension suppression pipeline.

use super::anchor_index::AnchorInfo;
use super::check_b_coverage::{
    build_adapter_coverage, build_adapter_reachable_targets, AdapterCoverage,
};
use super::pub_fns::PubFnInfo;
use super::workspace_graph::{canonical_name_for_pub_fn, CallGraph};
use super::HandlerTouchpoints;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::{HashMap, HashSet};

// qual:api
/// Emit one `CallParityMissingAdapter` finding per target pub-fn whose
/// boundary-reach set isn't symmetric across the configured adapters.
/// Integration: builds per-adapter coverage + the adapter-reachable
/// target set, then per-target finding construction via `inspect_target`.
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
    let reachable = build_adapter_reachable_targets(&coverage, graph, &cp.target);
    let ctx = TargetCtx {
        cp,
        coverage: &coverage,
        reachable: &reachable,
    };
    let mut out = Vec::new();
    for info in targets {
        // Concrete impl-methods of a trait whose anchor is enumerated
        // as target capability are skipped: the anchor takes
        // capability responsibility, otherwise an adapter that only
        // dispatches via `dyn Trait.method()` would silently produce
        // a false orphan finding for every concrete impl-method that
        // never appears in the touchpoint set.
        let canonical = canonical_name_for_pub_fn(info);
        if graph.is_anchor_backed_concrete(&canonical, &cp.target, &cp.adapters) {
            continue;
        }
        if let Some(hit) = inspect_target(info, &ctx) {
            out.push(hit);
        }
    }
    for (anchor, info) in graph.target_anchor_capabilities(&cp.target, &cp.adapters) {
        if let Some(hit) = inspect_anchor(anchor, info, &ctx) {
            out.push(hit);
        }
    }
    out
}

/// Same coverage check as `inspect_target`, but for synthetic
/// trait-method anchors. The anchor canonical is the capability —
/// `dyn Trait.method()` dispatch is what the boundary walker registers
/// as the touchpoint, so anchor coverage is decided purely by anchor
/// presence in the per-adapter coverage set. Direct calls to concrete
/// impl-methods (`LoggingHandler::handle()` straight on a struct
/// receiver) emit a different concrete edge and are inspected by
/// `inspect_target` against the concrete `pub_fns_by_layer[target]`
/// entry — they are NOT folded into anchor coverage here. This means
/// a workspace where `cli` dispatches via `dyn Trait` and `mcp` calls
/// the concrete impl directly produces two findings (one anchor
/// mismatch + one concrete-fn mismatch) for the same logical drift.
/// Cross-form synonym handling is intentionally left out — it would
/// require a graph-level synonym index and design discussion before
/// implementation. Operation: probe coverage on the anchor canonical.
fn inspect_anchor(anchor: &str, info: &AnchorInfo, ctx: &TargetCtx<'_>) -> Option<MatchLocation> {
    if is_excluded(anchor, ctx.cp) {
        return None;
    }
    let reached = adapters_reaching(anchor, ctx.coverage, &ctx.cp.adapters);
    let missing = adapters_missing(&reached, &ctx.cp.adapters);
    if missing.is_empty() {
        return None;
    }
    if reached.is_empty() && ctx.reachable.contains(anchor) {
        return None;
    }
    // Anchor findings need a real source location for suppression-window
    // matching, the orphan detector, and SARIF `startLine` validity. If
    // the type index didn't capture a span (synthetic fixtures, edge
    // cases), suppressing the finding is preferable to emitting one
    // with line=0 that silently won't match any `qual:allow` window.
    let location = info.location.as_ref()?;
    let mut reached = reached;
    reached.sort();
    Some(MatchLocation {
        file: location.file.clone(),
        line: location.line,
        column: location.column,
        kind: ViolationKind::CallParityMissingAdapter {
            target_fn: anchor.to_string(),
            target_layer: ctx.cp.target.clone(),
            reached_adapters: reached,
            missing_adapters: missing,
        },
    })
}

/// Read-only context bundle threaded into `inspect_target`. Operation:
/// data container.
struct TargetCtx<'a> {
    cp: &'a CompiledCallParity,
    coverage: &'a AdapterCoverage,
    reachable: &'a HashSet<String>,
}

/// Decide whether one target pub-fn produces a finding under the
/// boundary semantic. Returns `Some(hit)` for mismatch or orphan,
/// `None` otherwise (silent for excluded targets and post-boundary
/// helpers wired into adapter coverage).
/// Integration: probe coverage + suppress post-boundary plumbing.
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
    if reached.is_empty() && ctx.reachable.contains(&canonical) {
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
