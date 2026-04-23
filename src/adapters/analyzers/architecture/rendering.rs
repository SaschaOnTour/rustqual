//! Project `MatchLocation` values into cross-dimension `Finding`s.
//!
//! Every architecture rule emits `MatchLocation` (file/line + rich
//! `ViolationKind`). The public `Finding` contract is uniform across
//! dimensions — one line of message + metadata. This module owns the
//! translation so the analyzer file stays small and each ViolationKind's
//! rendering lives in one place.

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::config::architecture::SymbolPattern;
use crate::domain::{Dimension, Finding, Severity};

/// Project one `MatchLocation` into a `Finding` using the given rule id.
/// Operation: message formatting + field copy.
pub(super) fn match_to_finding(
    hit: MatchLocation,
    rule_id: &str,
    pattern: &SymbolPattern,
) -> Finding {
    Finding {
        file: hit.file,
        line: hit.line,
        column: hit.column,
        dimension: Dimension::Architecture,
        rule_id: rule_id.to_string(),
        message: format_match_message(&hit.kind, &pattern.reason),
        severity: Severity::Medium,
        ..Finding::default()
    }
}

/// Render a concise message from a `ViolationKind` plus the rule reason.
/// Integration: match-dispatch delegation to per-variant formatters.
pub(super) fn format_match_message(kind: &ViolationKind, reason: &str) -> String {
    let head = render_violation_head(kind);
    format!("{head}: {reason}")
}

/// Variant-specific head text for a `ViolationKind`.
/// Integration: match-dispatch delegation per variant kind.
fn render_violation_head(kind: &ViolationKind) -> String {
    match kind {
        ViolationKind::PathPrefix { rendered_path, .. } => format!("path \"{rendered_path}\""),
        ViolationKind::GlobImport { base_path } => format!("glob import {base_path}::*"),
        ViolationKind::MethodCall { name, syntax } => format!("{syntax} method call {name}"),
        ViolationKind::MacroCall { name } => format!("macro {name}!"),
        ViolationKind::FunctionCall { rendered_path } => format!("call {rendered_path}"),
        ViolationKind::LayerViolation {
            from_layer,
            to_layer,
            imported_path,
        } => format!("layer {from_layer} ↛ {to_layer} via {imported_path}"),
        ViolationKind::UnmatchedLayer { file } => format!("unmatched file {file}"),
        ViolationKind::ForbiddenEdge { imported_path, .. } => {
            format!("forbidden import {imported_path}")
        }
        ViolationKind::ItemKind { kind, name } => render_item_kind_head(kind, name),
        ViolationKind::Derive {
            trait_name,
            item_name,
        } => format!("derive({trait_name}) on {item_name}"),
        ViolationKind::TraitContract {
            trait_name,
            check,
            detail,
        } => format!("trait {trait_name} [{check}]: {detail}"),
        ViolationKind::CallParityNoDelegation { .. }
        | ViolationKind::CallParityMissingAdapter { .. } => render_call_parity_head(kind),
    }
}

/// Head text for the two call-parity ViolationKinds. Kept separate from
/// `render_violation_head` to keep that function below the cyclomatic
/// threshold — call-parity has two variants, each with its own shape.
/// Operation: variant-local formatting.
fn render_call_parity_head(kind: &ViolationKind) -> String {
    match kind {
        ViolationKind::CallParityNoDelegation {
            fn_name,
            adapter_layer,
            target_layer,
            call_depth,
        } => format!(
            "adapter {adapter_layer}::{fn_name} does not delegate to '{target_layer}' within {call_depth} hops"
        ),
        ViolationKind::CallParityMissingAdapter {
            target_fn,
            missing_adapters,
            ..
        } => format!(
            "'{target_fn}' is not reached from adapter layer(s): {}",
            missing_adapters.join(", ")
        ),
        _ => String::new(),
    }
}

/// Head text for an `ItemKind` violation (anonymous items omit the name).
/// Operation: conditional formatting.
fn render_item_kind_head(kind: &str, name: &str) -> String {
    if name.is_empty() {
        kind.to_string()
    } else {
        format!("{kind} {name}")
    }
}
