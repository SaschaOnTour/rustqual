//! `--explain <file>` diagnostic output.
//!
//! Given one parsed file and a compiled architecture config, produce a
//! structured report that shows:
//!   - the file's layer assignment (or re-export-point flag, or unmatched),
//!   - every `use` import classified (crate-internal, stdlib, external),
//!   - resolved target layers per import (when known),
//!   - the layer-rule and forbidden-rule hits that apply.
//!
//! The data shape (`ExplainReport`) is the testable contract; `render` turns
//! it into the text the CLI prints.

#![allow(dead_code)]

use crate::adapters::analyzers::architecture::compiled::CompiledArchitecture;
use crate::adapters::analyzers::architecture::forbidden_rule::{
    check_forbidden_rules, CompiledForbiddenRule,
};
use crate::adapters::analyzers::architecture::layer_rule::{
    check_layer_rule, LayerRuleInput, UnmatchedBehavior,
};
use crate::adapters::analyzers::architecture::use_tree::gather_imports;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::fmt::Write;

/// Complete explain report for one file.
#[derive(Debug)]
pub struct ExplainReport {
    pub file: String,
    pub layer: Option<String>,
    pub rank: Option<usize>,
    pub is_reexport: bool,
    pub imports: Vec<ImportEntry>,
    pub layer_violations: Vec<MatchLocation>,
    pub forbidden_violations: Vec<MatchLocation>,
}

/// One classified import in the file.
#[derive(Debug)]
pub struct ImportEntry {
    pub line: usize,
    pub rendered: String,
    pub kind: ImportKind,
}

/// Classification of the first-segment of an import.
#[derive(Debug)]
pub enum ImportKind {
    /// `use self::…`, `use super::…`, `use std::…`, `use core::…`, `use alloc::…`.
    Ignored { first_segment: String },
    /// `use crate::<seg>::…` — resolved layer if `<seg>` matches a layer glob.
    CrateInternal {
        target_segment: String,
        target_layer: Option<String>,
    },
    /// External crate — resolved via exact map or glob list if configured.
    ExternalCrate {
        crate_name: String,
        resolved_layer: Option<String>,
    },
}

/// Build the report for `path` using `compiled` as the rule source.
/// Integration: delegates to classification, rule-running, and assembly ops.
pub fn explain_file(path: &str, ast: &syn::File, compiled: &CompiledArchitecture) -> ExplainReport {
    let is_reexport = compiled.reexport_points.is_match(path);
    let (layer, rank) = classify_file_layer(path, compiled);
    let imports = classify_imports(ast, compiled);
    let (layer_violations, forbidden_violations) = collect_rule_hits(path, ast, compiled);
    ExplainReport {
        file: path.to_string(),
        layer,
        rank,
        is_reexport,
        imports,
        layer_violations,
        forbidden_violations,
    }
}

/// Determine which layer the file belongs to.
/// Operation: glob-based lookup.
fn classify_file_layer(
    path: &str,
    compiled: &CompiledArchitecture,
) -> (Option<String>, Option<usize>) {
    let layer = compiled.layers.layer_for_file(path).map(str::to_string);
    let rank = layer.as_deref().and_then(|l| compiled.layers.rank_of(l));
    (layer, rank)
}

/// Walk the file's imports and classify each leaf.
/// Operation: iterator-chain classification.
fn classify_imports(ast: &syn::File, compiled: &CompiledArchitecture) -> Vec<ImportEntry> {
    gather_imports(ast)
        .into_iter()
        .map(|(segments, span)| ImportEntry {
            line: span.start().line,
            rendered: segments.join("::"),
            kind: classify_segments(&segments, compiled),
        })
        .collect()
}

/// Decide the ImportKind for a segment list.
/// Integration: match-dispatch delegation over the first segment.
fn classify_segments(segments: &[String], compiled: &CompiledArchitecture) -> ImportKind {
    let Some(first) = segments.first() else {
        return ImportKind::Ignored {
            first_segment: String::new(),
        };
    };
    match first.as_str() {
        "self" | "super" | "std" | "core" | "alloc" => ImportKind::Ignored {
            first_segment: first.clone(),
        },
        "crate" => classify_crate_import(segments, compiled),
        _ => classify_external_import(first, compiled),
    }
}

/// Classify a `crate::<seg>::…` import.
/// Operation: segment lookup + layer resolution.
fn classify_crate_import(segments: &[String], compiled: &CompiledArchitecture) -> ImportKind {
    let seg = segments.get(1).cloned().unwrap_or_default();
    let target_layer = compiled
        .layers
        .layer_for_crate_segment(&seg)
        .map(str::to_string);
    ImportKind::CrateInternal {
        target_segment: seg,
        target_layer,
    }
}

/// Classify an external-crate import; check exact map then glob list.
/// Operation: two-step resolution logic.
fn classify_external_import(crate_name: &str, compiled: &CompiledArchitecture) -> ImportKind {
    if let Some(layer) = compiled.external_exact.get(crate_name) {
        return ImportKind::ExternalCrate {
            crate_name: crate_name.to_string(),
            resolved_layer: Some(layer.clone()),
        };
    }
    let resolved_layer = compiled
        .external_glob
        .iter()
        .find(|(m, _)| m.is_match(crate_name))
        .map(|(_, l)| l.clone());
    ImportKind::ExternalCrate {
        crate_name: crate_name.to_string(),
        resolved_layer,
    }
}

/// Run the layer and forbidden rules against this single file.
/// Integration: delegates to each rule checker via a single-entry slice.
fn collect_rule_hits(
    path: &str,
    ast: &syn::File,
    compiled: &CompiledArchitecture,
) -> (Vec<MatchLocation>, Vec<MatchLocation>) {
    let files = [(path.to_string(), ast)];
    let layer_hits = run_layer_rule(&files, compiled);
    let forbidden_hits = run_forbidden_rules(&files, &compiled.forbidden);
    (layer_hits, forbidden_hits)
}

/// Invoke the layer rule on a single-file slice.
/// Operation: wraps the checker call with an owned-refs slice.
fn run_layer_rule(
    files: &[(String, &syn::File)],
    compiled: &CompiledArchitecture,
) -> Vec<MatchLocation> {
    let input = LayerRuleInput {
        layers: &compiled.layers,
        reexport_points: &compiled.reexport_points,
        unmatched_behavior: compiled.unmatched_behavior,
        external_exact: &compiled.external_exact,
        external_glob: &compiled.external_glob,
    };
    check_layer_rule(files, &input)
}

/// Invoke the forbidden rules on a single-file slice.
/// Trivial: delegates to the checker.
fn run_forbidden_rules(
    files: &[(String, &syn::File)],
    rules: &[CompiledForbiddenRule],
) -> Vec<MatchLocation> {
    check_forbidden_rules(files, rules)
}

// ── rendering ──────────────────────────────────────────────────────────

impl ExplainReport {
    /// Render the report as human-readable text.
    /// Integration: delegates to per-section printers via a String buffer.
    pub fn render(&self) -> String {
        let mut out = String::new();
        write_header(&mut out, self);
        write_imports(&mut out, self);
        write_violations(&mut out, self);
        out
    }
}

/// Emit the header block (file, layer or unmatched, re-export flag).
/// Operation: formatted writes.
fn write_header(out: &mut String, r: &ExplainReport) {
    let _ = writeln!(out, "═══ Architecture Explain: {} ═══", r.file);
    match (&r.layer, r.rank, r.is_reexport) {
        (_, _, true) => {
            let _ = writeln!(out, "Status: re-export point (rules bypassed)");
        }
        (Some(l), Some(rank), _) => {
            let _ = writeln!(out, "Layer: {l} (rank {rank})");
        }
        _ => {
            let _ = writeln!(out, "Layer: <unmatched>");
        }
    }
}

/// Emit the imports block.
/// Operation: iterator-chain writes, no logic.
fn write_imports(out: &mut String, r: &ExplainReport) {
    let _ = writeln!(out, "\nImports ({}):", r.imports.len());
    r.imports.iter().for_each(|i| write_import_entry(out, i));
}

/// Emit a single import entry, rendering its classification.
/// Operation: match-dispatch formatting.
fn write_import_entry(out: &mut String, i: &ImportEntry) {
    let tail = render_import_tail(&i.kind);
    let _ = writeln!(out, "  line {}: {} — {}", i.line, i.rendered, tail);
}

/// Describe an `ImportKind` in one short phrase.
/// Operation: pattern-match on the enum.
fn render_import_tail(kind: &ImportKind) -> String {
    match kind {
        ImportKind::Ignored { first_segment } => format!("ignored ({first_segment})"),
        ImportKind::CrateInternal {
            target_segment,
            target_layer,
        } => match target_layer {
            Some(l) => format!("crate::{target_segment} → layer {l}"),
            None => format!("crate::{target_segment} → unresolved"),
        },
        ImportKind::ExternalCrate {
            crate_name,
            resolved_layer,
        } => match resolved_layer {
            Some(l) => format!("external {crate_name} → layer {l}"),
            None => format!("external {crate_name} → no mapping"),
        },
    }
}

/// Emit the layer- and forbidden-rule violation blocks.
/// Operation: conditional section writes.
fn write_violations(out: &mut String, r: &ExplainReport) {
    write_layer_violations(out, &r.layer_violations);
    write_forbidden_violations(out, &r.forbidden_violations);
    if r.layer_violations.is_empty() && r.forbidden_violations.is_empty() {
        let _ = writeln!(out, "\n✓ No architecture violations.");
    }
}

/// Emit the layer-rule violation block (if any).
/// Operation: iterator-chain over filtered kinds.
fn write_layer_violations(out: &mut String, hits: &[MatchLocation]) {
    if hits.is_empty() {
        return;
    }
    let _ = writeln!(out, "\nLayer violations:");
    hits.iter().for_each(|h| write_layer_line(out, h));
}

/// Emit one layer-violation line.
/// Operation: match-dispatch over ViolationKind.
fn write_layer_line(out: &mut String, h: &MatchLocation) {
    if let ViolationKind::LayerViolation {
        from_layer,
        to_layer,
        imported_path,
    } = &h.kind
    {
        let _ = writeln!(
            out,
            "  line {}: {from_layer} ↛ {to_layer}  via {imported_path}",
            h.line
        );
    }
}

/// Emit the forbidden-rule violation block (if any).
/// Operation: iterator-chain writes.
fn write_forbidden_violations(out: &mut String, hits: &[MatchLocation]) {
    if hits.is_empty() {
        return;
    }
    let _ = writeln!(out, "\nForbidden rule violations:");
    hits.iter().for_each(|h| write_forbidden_line(out, h));
}

/// Emit one forbidden-violation line.
/// Operation: match-dispatch over ViolationKind.
fn write_forbidden_line(out: &mut String, h: &MatchLocation) {
    if let ViolationKind::ForbiddenEdge {
        reason,
        imported_path,
    } = &h.kind
    {
        let _ = writeln!(out, "  line {}: {imported_path}  [{reason}]", h.line);
    }
}
