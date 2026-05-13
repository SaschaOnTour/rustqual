//! Check A — Adapter-must-delegate.
//!
//! Every `pub fn` in a configured adapter layer must reach at least one
//! fn in the configured target layer (within `call_depth` adapter-
//! internal hops). A fn that satisfies this delegates to the shared
//! Application layer; a fn that fails has almost certainly inlined
//! business logic.
//!
//! Reads from the shared `HandlerTouchpoints` cache built by
//! `mod::build_handler_touchpoints` — the cache already encodes the
//! same forward-BFS-with-boundary-stop semantic Check A needs, so we
//! only need to test whether a handler's touchpoint set is empty.
//! Deprecated handlers are filtered upstream (absent from the cache)
//! and silently skipped here.

use super::pub_fns::PubFnInfo;
use super::workspace_graph::canonical_name_for_pub_fn;
use super::HandlerTouchpoints;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::HashMap;

// qual:api
/// Emit one `CallParityNoDelegation` finding per adapter pub-fn whose
/// touchpoint set is empty.
/// Integration: per-adapter scan + per-fn cache lookup via `inspect_handler`.
pub(crate) fn check_no_delegation<'ast>(
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'ast>>>,
    touchpoints: &HandlerTouchpoints,
    cp: &CompiledCallParity,
) -> Vec<MatchLocation> {
    let mut out = Vec::new();
    for adapter_layer in &cp.adapters {
        let Some(fns) = pub_fns_by_layer.get(adapter_layer) else {
            continue;
        };
        for info in fns {
            if let Some(hit) = inspect_handler(info, adapter_layer, touchpoints, cp) {
                out.push(hit);
            }
        }
    }
    out
}

/// Decide whether one adapter pub-fn produces a Check A finding.
/// Returns `Some(hit)` only when the cache has an entry for the
/// handler with an empty touchpoint set; deprecated handlers are
/// absent from the cache entirely and silently skipped.
/// Operation: per-handler probe.
fn inspect_handler(
    info: &PubFnInfo<'_>,
    adapter_layer: &str,
    touchpoints: &HandlerTouchpoints,
    cp: &CompiledCallParity,
) -> Option<MatchLocation> {
    let canonical = canonical_name_for_pub_fn(info);
    let tps = touchpoints.get(&canonical)?;
    if !tps.is_empty() {
        return None;
    }
    Some(MatchLocation {
        file: info.file.clone(),
        line: info.line,
        column: 0,
        kind: ViolationKind::CallParityNoDelegation {
            fn_name: info.fn_name.clone(),
            adapter_layer: adapter_layer.to_string(),
            target_layer: cp.target.clone(),
            call_depth: cp.call_depth,
        },
    })
}
