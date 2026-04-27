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
use crate::ports::AnalysisContext;

/// Build `(path, &ast)` refs for every parsed file — the shape every
/// workspace-wide architecture rule expects. Extracting this keeps the
/// per-rule `collect_findings` functions focused on their actual work.
pub(crate) fn build_file_refs<'a>(ctx: &'a AnalysisContext<'_>) -> Vec<(String, &'a syn::File)> {
    ctx.files.iter().map(|f| (f.path.clone(), &f.ast)).collect()
}

/// Shared Finding-construction for every architecture sub-rule.
/// Each sub-rule picks its own rule_id scheme + severity but fills the
/// same cross-dimension `Finding` shape; centralising the construction
/// keeps them consistent.
pub(crate) fn build_architecture_finding(
    hit: MatchLocation,
    rule_id: String,
    reason: &str,
    severity: Severity,
) -> Finding {
    let message = format_match_message(&hit.kind, reason);
    Finding {
        file: hit.file,
        line: hit.line,
        column: hit.column,
        dimension: Dimension::Architecture,
        rule_id,
        message,
        severity,
        ..Finding::default()
    }
}

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
        | ViolationKind::CallParityMissingAdapter { .. }
        | ViolationKind::CallParityMultiplicityMismatch { .. }
        | ViolationKind::CallParityMultiTouchpoint { .. } => render_call_parity_head(kind),
    }
}

/// Head text for the four call-parity ViolationKinds. Kept separate
/// from `render_violation_head` to keep that function below the
/// cyclomatic threshold.
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
        ViolationKind::CallParityMultiplicityMismatch {
            target_fn,
            counts_per_adapter,
            ..
        } => {
            let parts: Vec<String> = counts_per_adapter
                .iter()
                .map(|(a, c)| format!("{a}={c}"))
                .collect();
            format!(
                "'{target_fn}' has divergent handler counts across adapters: {}",
                parts.join(", ")
            )
        }
        ViolationKind::CallParityMultiTouchpoint {
            fn_name,
            adapter_layer,
            touchpoints,
        } => format!(
            "adapter {adapter_layer}::{fn_name} has multiple target touchpoints: {}",
            touchpoints.join(", ")
        ),
        // Caller (`render_violation_head`) only dispatches CallParity*
        // variants here. Reaching this branch would mean a new variant
        // was added without updating the dispatcher.
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
