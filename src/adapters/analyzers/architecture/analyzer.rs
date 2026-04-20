//! `ArchitectureAnalyzer` — implements the `DimensionAnalyzer` port.
//!
//! Runs every rule type configured under `[architecture]` against the
//! parsed workspace and projects the rich `MatchLocation` outputs into
//! cross-dimension `Finding`s. Symbol patterns honour their
//! `allowed_in` / `forbidden_in` scope globs; the layer and forbidden
//! rules are inherently workspace-wide.
//!
//! The adapter is state-less — one instance per run is sufficient. The
//! compiled rule structures are rebuilt on every `analyze` call; that
//! keeps the port contract minimal (no setup step) at the cost of
//! re-compiling globs when the Application layer calls back multiple
//! times, which it currently does not.

use crate::adapters::analyzers::architecture::compiled::{
    compile_architecture, CompiledArchitecture,
};
use crate::adapters::analyzers::architecture::forbidden_rule::{
    check_forbidden_rules, CompiledForbiddenRule,
};
use crate::adapters::analyzers::architecture::layer_rule::{check_layer_rule, LayerRuleInput};
use crate::adapters::analyzers::architecture::matcher::{
    find_derive_matches, find_function_call_matches, find_glob_imports, find_item_kind_matches,
    find_macro_calls, find_method_call_matches, find_path_prefix_matches,
};
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::config::architecture::SymbolPattern;
use crate::domain::{Dimension, Finding, Severity};
use crate::ports::{AnalysisContext, DimensionAnalyzer};
use globset::{Glob, GlobSet, GlobSetBuilder};

/// DimensionAnalyzer adapter for the Architecture dimension.
pub struct ArchitectureAnalyzer;

impl DimensionAnalyzer for ArchitectureAnalyzer {
    fn dimension_name(&self) -> &'static str {
        "architecture"
    }

    /// Integration: delegate per-rule-type work and collect findings.
    fn analyze(&self, ctx: &AnalysisContext<'_>) -> Vec<Finding> {
        let arch = &ctx.config.architecture;
        if !arch.enabled {
            return Vec::new();
        }
        let compiled = match compile_architecture(arch) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error compiling [architecture] config: {e}");
                return Vec::new();
            }
        };
        collect_all_findings(ctx, arch, &compiled)
    }
}

/// Gather findings from every rule type.
/// Integration: sums per-rule-type sub-collections.
fn collect_all_findings(
    ctx: &AnalysisContext<'_>,
    arch: &crate::config::ArchitectureConfig,
    compiled: &CompiledArchitecture,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(collect_symbol_findings(ctx, &arch.patterns));
    findings.extend(collect_layer_findings(ctx, compiled));
    findings.extend(collect_forbidden_findings(ctx, &compiled.forbidden));
    findings.extend(
        crate::adapters::analyzers::architecture::trait_contract_rule::collect_findings(
            ctx,
            &compiled.trait_contracts,
            format_match_message,
        ),
    );
    findings
}

// ── symbol patterns ────────────────────────────────────────────────────

/// Run every `[[architecture.pattern]]` entry on every file.
/// Operation: iterator-chain over patterns and files.
fn collect_symbol_findings(ctx: &AnalysisContext<'_>, patterns: &[SymbolPattern]) -> Vec<Finding> {
    patterns
        .iter()
        .flat_map(|p| collect_pattern_findings(ctx, p))
        .collect()
}

/// Run one symbol pattern against every in-scope file.
/// Integration: compiles scope globs, iterates files, delegates to matcher driver.
fn collect_pattern_findings(ctx: &AnalysisContext<'_>, pattern: &SymbolPattern) -> Vec<Finding> {
    let Some(scope) = compile_pattern_scope(pattern) else {
        return Vec::new();
    };
    ctx.files
        .iter()
        .filter(|f| scope.accepts(&f.path))
        .flat_map(|f| run_pattern_matchers(f, pattern))
        .collect()
}

/// Compiled scope decision for one pattern.
struct PatternScope {
    kind: ScopeKind,
    paths: GlobSet,
    except: GlobSet,
}

/// Whitelist or blocklist interpretation of `paths`.
enum ScopeKind {
    AllowedIn,
    ForbiddenIn,
}

impl PatternScope {
    /// True when the pattern applies to `path` (i.e. matchers should run).
    /// Operation: glob-lookup logic.
    fn accepts(&self, path: &str) -> bool {
        if self.except.is_match(path) {
            return false;
        }
        match self.kind {
            ScopeKind::AllowedIn => !self.paths.is_match(path),
            ScopeKind::ForbiddenIn => self.paths.is_match(path),
        }
    }
}

/// Compile a pattern's scope fields into matching globs.
/// Operation: XOR validation + glob compilation.
fn compile_pattern_scope(pattern: &SymbolPattern) -> Option<PatternScope> {
    let (kind, raw_paths) = match (&pattern.allowed_in, &pattern.forbidden_in) {
        (Some(p), None) => (ScopeKind::AllowedIn, p.as_slice()),
        (None, Some(p)) => (ScopeKind::ForbiddenIn, p.as_slice()),
        _ => return None,
    };
    let paths = build_globset(raw_paths)?;
    let except = build_globset(&pattern.except).unwrap_or_else(GlobSet::empty);
    Some(PatternScope {
        kind,
        paths,
        except,
    })
}

/// Build a GlobSet from string patterns; returns None if any is invalid.
/// Operation: per-pattern add with error short-circuit.
fn build_globset(patterns: &[String]) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for p in patterns {
        match Glob::new(p) {
            Ok(g) => {
                builder.add(g);
            }
            Err(e) => {
                eprintln!("architecture: invalid glob \"{p}\": {e}");
                return None;
            }
        }
    }
    builder.build().ok()
}

/// Run every active matcher of `pattern` on one parsed file.
/// Integration: iterator-chain over matchers, collects findings.
fn run_pattern_matchers(file: &crate::ports::ParsedFile, pattern: &SymbolPattern) -> Vec<Finding> {
    let rule_id = format!("architecture/pattern/{}", pattern.name);
    let mut out = Vec::new();

    if let Some(prefixes) = &pattern.forbid_path_prefix {
        let hits = find_path_prefix_matches(&file.path, &file.ast, prefixes);
        out.extend(
            hits.into_iter()
                .map(|h| match_to_finding(h, &rule_id, pattern)),
        );
    }
    if let Some(names) = &pattern.forbid_method_call {
        let hits = find_method_call_matches(&file.path, &file.ast, names);
        out.extend(
            hits.into_iter()
                .map(|h| match_to_finding(h, &rule_id, pattern)),
        );
    }
    if let Some(paths) = &pattern.forbid_function_call {
        let hits = find_function_call_matches(&file.path, &file.ast, paths);
        out.extend(
            hits.into_iter()
                .map(|h| match_to_finding(h, &rule_id, pattern)),
        );
    }
    if let Some(names) = &pattern.forbid_macro_call {
        let hits = find_macro_calls(&file.path, &file.ast, names);
        out.extend(
            hits.into_iter()
                .map(|h| match_to_finding(h, &rule_id, pattern)),
        );
    }
    if matches!(pattern.forbid_glob_import, Some(true)) {
        let hits = find_glob_imports(&file.path, &file.ast);
        out.extend(
            hits.into_iter()
                .map(|h| match_to_finding(h, &rule_id, pattern)),
        );
    }
    if let Some(kinds) = &pattern.forbid_item_kind {
        let hits = find_item_kind_matches(&file.path, &file.ast, kinds);
        out.extend(
            hits.into_iter()
                .map(|h| match_to_finding(h, &rule_id, pattern)),
        );
    }
    if let Some(names) = &pattern.forbid_derive {
        let hits = find_derive_matches(&file.path, &file.ast, names);
        out.extend(
            hits.into_iter()
                .map(|h| match_to_finding(h, &rule_id, pattern)),
        );
    }
    out
}

/// Project one `MatchLocation` into a `Finding` using the given rule id.
/// Operation: message formatting + field copy.
fn match_to_finding(hit: MatchLocation, rule_id: &str, pattern: &SymbolPattern) -> Finding {
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
fn format_match_message(kind: &ViolationKind, reason: &str) -> String {
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

// ── layer rule ─────────────────────────────────────────────────────────

/// Run the layer rule against the whole parsed workspace.
/// Operation: workspace projection + checker call + mapping.
fn collect_layer_findings(
    ctx: &AnalysisContext<'_>,
    compiled: &CompiledArchitecture,
) -> Vec<Finding> {
    let refs: Vec<(String, &syn::File)> =
        ctx.files.iter().map(|f| (f.path.clone(), &f.ast)).collect();
    let input = LayerRuleInput {
        layers: &compiled.layers,
        reexport_points: &compiled.reexport_points,
        unmatched_behavior: compiled.unmatched_behavior,
        external_exact: &compiled.external_exact,
        external_glob: &compiled.external_glob,
    };
    check_layer_rule(&refs, &input)
        .into_iter()
        .map(layer_hit_to_finding)
        .collect()
}

/// Project a layer/unmatched `MatchLocation` into a Finding.
/// Operation: rule_id selection + field copy.
fn layer_hit_to_finding(hit: MatchLocation) -> Finding {
    let rule_id = match &hit.kind {
        ViolationKind::UnmatchedLayer { .. } => "architecture/layer/unmatched",
        _ => "architecture/layer",
    };
    Finding {
        file: hit.file.clone(),
        line: hit.line,
        column: hit.column,
        dimension: Dimension::Architecture,
        rule_id: rule_id.to_string(),
        message: format_match_message(&hit.kind, "layer import rule"),
        severity: Severity::High,
        ..Finding::default()
    }
}

// ── forbidden rules ────────────────────────────────────────────────────

/// Run every forbidden rule and project its hits into findings.
/// Operation: workspace projection + checker call + mapping.
fn collect_forbidden_findings(
    ctx: &AnalysisContext<'_>,
    rules: &[CompiledForbiddenRule],
) -> Vec<Finding> {
    if rules.is_empty() {
        return Vec::new();
    }
    let refs: Vec<(String, &syn::File)> =
        ctx.files.iter().map(|f| (f.path.clone(), &f.ast)).collect();
    check_forbidden_rules(&refs, rules)
        .into_iter()
        .map(forbidden_hit_to_finding)
        .collect()
}

/// Project a forbidden-edge hit into a Finding.
/// Operation: field copy with dimension rule_id.
fn forbidden_hit_to_finding(hit: MatchLocation) -> Finding {
    let reason = if let ViolationKind::ForbiddenEdge { reason, .. } = &hit.kind {
        reason.clone()
    } else {
        String::new()
    };
    Finding {
        file: hit.file.clone(),
        line: hit.line,
        column: hit.column,
        dimension: Dimension::Architecture,
        rule_id: "architecture/forbidden".to_string(),
        message: format_match_message(&hit.kind, &reason),
        severity: Severity::High,
        ..Finding::default()
    }
}
