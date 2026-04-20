//! `[[architecture.trait_contract]]` — check trait definitions in scope
//! against a suite of structural rules.
//!
//! `error_variant` looks up the error type in the same file where the
//! trait is defined; cross-file resolution is out of scope.

mod checks;
mod rendering;

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::domain::{Dimension, Finding, Severity};
use crate::ports::AnalysisContext;
use checks::{
    check_async, check_error_variants, check_object_safety, check_receiver, check_required_param,
    check_return_type, check_supertraits, TraitSite,
};
use globset::GlobSet;

/// A compiled trait-contract rule, ready to run against parsed files.
#[derive(Debug)]
pub struct CompiledTraitContract {
    pub name: String,
    pub scope: GlobSet,
    pub receiver_may_be: Option<Vec<String>>,
    pub required_param_type_contains: Option<String>,
    pub forbidden_return_type_contains: Vec<String>,
    pub forbidden_error_variant_contains: Vec<String>,
    pub error_types: Vec<String>,
    pub methods_must_be_async: Option<bool>,
    pub must_be_object_safe: Option<bool>,
    pub required_supertraits_contain: Vec<String>,
}

/// Check every trait definition in scope against each compiled rule.
/// Integration: iterator-chain over files × rules × traits.
pub fn check_trait_contracts(
    files: &[(String, &syn::File)],
    rules: &[CompiledTraitContract],
) -> Vec<MatchLocation> {
    files
        .iter()
        .flat_map(|(path, ast)| check_file(path, ast, rules))
        .collect()
}

/// Run every rule against every trait in one file.
/// Integration: dispatches per trait + per rule.
fn check_file(path: &str, ast: &syn::File, rules: &[CompiledTraitContract]) -> Vec<MatchLocation> {
    let traits: Vec<&syn::ItemTrait> = ast
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Trait(t) => Some(t),
            _ => None,
        })
        .collect();
    rules
        .iter()
        .filter(|r| r.scope.is_match(path))
        .flat_map(|r| {
            traits
                .iter()
                .flat_map(move |t| check_trait(path, t, ast, r))
        })
        .collect()
}

/// Run each enabled check against one trait.
/// Integration: delegates to per-check helpers.
fn check_trait(
    path: &str,
    t: &syn::ItemTrait,
    ast: &syn::File,
    rule: &CompiledTraitContract,
) -> Vec<MatchLocation> {
    let methods = checks::trait_methods(t);
    let site = TraitSite {
        path,
        t,
        methods: &methods,
        ast,
    };
    let mut out = Vec::new();
    check_receiver(&site, rule, &mut out);
    check_async(&site, rule, &mut out);
    check_return_type(&site, rule, &mut out);
    check_required_param(&site, rule, &mut out);
    check_supertraits(&site, rule, &mut out);
    check_object_safety(&site, rule, &mut out);
    check_error_variants(&site, rule, &mut out);
    out
}

// qual:allow(dry) reason: "parallel to forbidden_rule's collect — each rule family owns its own mapping"
/// Run every trait-contract rule on the workspace and project into Findings.
/// Integration: delegates refs-build, check call, and per-hit mapping.
pub fn collect_findings<F>(
    ctx: &AnalysisContext<'_>,
    rules: &[CompiledTraitContract],
    format_message: F,
) -> Vec<Finding>
where
    F: Fn(&ViolationKind, &str) -> String,
{
    if rules.is_empty() {
        return Vec::new();
    }
    let refs: Vec<(String, &syn::File)> =
        ctx.files.iter().map(|f| (f.path.clone(), &f.ast)).collect();
    check_trait_contracts(&refs, rules)
        .into_iter()
        .map(|hit| hit_to_finding(hit, &format_message))
        .collect()
}

/// Project a trait-contract hit to a domain `Finding`.
/// Operation: rule_id selection + field copy.
fn hit_to_finding<F>(hit: MatchLocation, format_message: &F) -> Finding
where
    F: Fn(&ViolationKind, &str) -> String,
{
    let rule_id = match &hit.kind {
        ViolationKind::TraitContract { check, .. } => {
            format!("architecture/trait_contract/{check}")
        }
        _ => "architecture/trait_contract".to_string(),
    };
    let message = format_message(&hit.kind, "trait contract");
    Finding {
        file: hit.file,
        line: hit.line,
        column: hit.column,
        dimension: Dimension::Architecture,
        rule_id,
        message,
        severity: Severity::High,
        ..Finding::default()
    }
}
