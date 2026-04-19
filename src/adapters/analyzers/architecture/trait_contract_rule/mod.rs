//! `[[architecture.trait_contract]]` — check trait definitions in scope
//! against a suite of structural rules.
//!
//! Seven checks (Phase 8), each emitting a `ViolationKind::TraitContract`
//! with a short `check` identifier plus a human-readable detail:
//!
//!   - `receiver`       — method self-receiver form vs `receiver_may_be`
//!   - `async`          — methods must be `async fn`
//!   - `return_type`    — return types must not contain forbidden substrings
//!   - `required_param` — at least one parameter must contain a substring
//!   - `supertrait`     — trait's direct supertrait list must mention each required name
//!   - `object_safety`  — conservative: no `Self` return, no method-level generics
//!   - `error_variant`  — for the trait's error return type (by naming or
//!     explicit `error_types`), no enum variant contains a forbidden substring
//!
//! File-scoped: `error_variant` looks up the error type in the same file
//! where the trait is defined. Cross-file resolution is intentionally out
//! of scope for Phase 8.

#![allow(dead_code)]

mod checks;
mod rendering;

use crate::adapters::analyzers::architecture::MatchLocation;
use checks::{
    check_async, check_error_variants, check_object_safety, check_receiver, check_required_param,
    check_return_type, check_supertraits,
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
// qual:api
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
    let mut out = Vec::new();
    check_receiver(path, t, rule, &mut out);
    check_async(path, t, rule, &mut out);
    check_return_type(path, t, rule, &mut out);
    check_required_param(path, t, rule, &mut out);
    check_supertraits(path, t, rule, &mut out);
    check_object_safety(path, t, rule, &mut out);
    check_error_variants(path, t, ast, rule, &mut out);
    out
}
