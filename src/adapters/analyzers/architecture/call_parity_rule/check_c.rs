//! Check C — multi-touchpoint detection.
//!
//! Each adapter pub-fn should have **exactly one** touchpoint in the
//! target layer. A pub-fn with two or more touchpoints is orchestrating
//! across application calls itself — that orchestration logic risks
//! divergence between adapters and is the SRP-shaped smell that lets
//! cli and mcp silently take different paths through the application.
//!
//! Severity is configurable via `single_touchpoint`:
//!
//! - `Off`: skip the check entirely.
//! - `Warn` (default): emit at `Severity::Low`.
//! - `Error`: emit at `Severity::Medium`.
//!
//! `Off` short-circuits in `check_multi_touchpoint`. The Warn-vs-Error
//! distinction is applied at projection time in `mod.rs`.

use super::pub_fns::PubFnInfo;
use super::workspace_graph::canonical_name_for_pub_fn;
use super::HandlerTouchpoints;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::config::architecture::SingleTouchpointMode;
use std::collections::HashMap;

// qual:api
/// Emit one `CallParityMultiTouchpoint` finding per adapter pub-fn that
/// has more than one touchpoint in the target layer.
/// Integration: per-adapter scan + per-fn touchpoint lookup via
/// `inspect_handler`.
pub(crate) fn check_multi_touchpoint<'ast>(
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'ast>>>,
    touchpoints: &HandlerTouchpoints,
    cp: &CompiledCallParity,
) -> Vec<MatchLocation> {
    if cp.single_touchpoint == SingleTouchpointMode::Off {
        return Vec::new();
    }
    let mut out = Vec::new();
    for adapter in &cp.adapters {
        let Some(handlers) = pub_fns_by_layer.get(adapter) else {
            continue;
        };
        for info in handlers {
            if let Some(hit) = inspect_handler(info, adapter, touchpoints) {
                out.push(hit);
            }
        }
    }
    out
}

/// Decide whether one adapter pub-fn produces a Check C finding.
/// Returns `Some(hit)` only when its touchpoint set has size > 1.
/// Deprecated handlers are absent from the cache and silently skipped.
/// Operation: per-handler probe.
fn inspect_handler(
    info: &PubFnInfo<'_>,
    adapter_layer: &str,
    touchpoints: &HandlerTouchpoints,
) -> Option<MatchLocation> {
    let canonical = canonical_name_for_pub_fn(info);
    let tps = touchpoints.get(&canonical)?;
    if tps.len() < 2 {
        return None;
    }
    let mut sorted: Vec<String> = tps.iter().cloned().collect();
    sorted.sort();
    Some(MatchLocation {
        file: info.file.clone(),
        line: info.line,
        column: 0,
        kind: ViolationKind::CallParityMultiTouchpoint {
            fn_name: info.fn_name.clone(),
            adapter_layer: adapter_layer.to_string(),
            touchpoints: sorted,
        },
    })
}
