pub mod boilerplate;
pub(crate) mod call_targets;
pub mod dead_code;
pub mod dead_types;
pub(crate) mod declared_types;
pub mod fragments;
pub mod functions;
pub mod match_patterns;
pub(crate) mod split_names;
pub(crate) mod type_references;
pub mod wildcards;

pub use boilerplate::BoilerplateFind;
pub use dead_code::{DeadCodeKind, DeadCodeWarning};
pub use dead_types::{DeadTypeKind, DeadTypeWarning};
pub use fragments::FragmentGroup;
pub use functions::{DuplicateGroup, DuplicateKind};

use crate::adapters::shared::declared_function::DeclaredFunction;
use crate::adapters::shared::file_visitor::visit_all_files;
use crate::adapters::shared::normalize::NormalizedToken;

// ── Shared types ────────────────────────────────────────────────

/// A function with its normalized hash information, ready for duplicate detection.
pub struct FunctionHashEntry {
    pub name: String,
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
    pub hash: u64,
    pub token_count: usize,
    pub tokens: Vec<NormalizedToken>,
}

// ── Function hash collection ────────────────────────────────────

/// Collect function hashes from all parsed files.
/// Trivial: creates visitor and delegates to visit_all_files.
pub(crate) fn collect_function_hashes(
    parsed: &[(String, String, syn::File)],
    config: &crate::config::sections::DuplicatesConfig,
) -> Vec<FunctionHashEntry> {
    let mut collector = functions::FunctionCollector::new(config);
    visit_all_files(parsed, &mut collector);
    collector.entries
}

/// Collect declared function metadata from all parsed files.
/// Trivial: creates visitor and delegates to visit_all_files.
pub(crate) fn collect_declared_functions(
    parsed: &[(String, String, syn::File)],
) -> Vec<DeclaredFunction> {
    let mut collector = dead_code::DeclaredFnCollector::new();
    visit_all_files(parsed, &mut collector);
    collector.functions
}

/// Collect declared type and constant metadata from all parsed files, marking
/// those from test-only files as test code.
///
/// The propagation belongs here rather than at each call site: DRY-006 and the
/// stale-marker check both build their exemption from `is_test`, and when only
/// one of them applied it the two silently disagreed about every declaration in
/// an integration-test file.
/// Operation: collection + flag pass, own calls hidden in the closures.
pub(crate) fn collect_declared_types(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &std::collections::HashSet<String>,
) -> Vec<crate::adapters::shared::declared_type::DeclaredType> {
    let mut collector = declared_types::DeclaredTypeCollector::new();
    visit_all_files(parsed, &mut collector);
    let mut declared = collector.types;
    declared
        .iter_mut()
        .filter(|d| cfg_test_files.contains(&d.file))
        .for_each(|d| d.is_test = true);
    declared
}

// ── Attribute helpers ───────────────────────────────────────────

// `has_cfg_test` and `has_test_attr` live in `adapters::shared::cfg_test`
// (multi-dimension utility). Re-exports keep existing call sites working.
pub(crate) use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};

pub(crate) use type_references::collect_type_references;

/// Check if attributes contain `#[allow(..., dead_code, ...)]`. Handles
/// both the single-lint form (`#[allow(dead_code)]`) and the list form
/// (`#[allow(dead_code, unused_variables)]`).
/// Operation: attribute inspection + punctuated-path parsing.
fn has_allow_dead_code(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("allow"))
        .any(allow_contains_dead_code)
}

/// True if this `#[allow(...)]` attribute's argument list contains
/// `dead_code` as one of its (potentially many) path entries.
/// Operation: punctuated parse + any-match, no own calls.
fn allow_contains_dead_code(attr: &syn::Attribute) -> bool {
    attr.parse_args_with(syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated)
        .is_ok_and(|paths| paths.iter().any(|p| p.is_ident("dead_code")))
}

/// Build qualified name from optional parent type and base name.
/// Operation: simple string formatting, no own calls.
fn qualify_name(parent: &Option<String>, name: &str) -> String {
    parent
        .as_ref()
        .map_or_else(|| name.to_string(), |p| [p.as_str(), "::", name].concat())
}

#[cfg(test)]
mod tests;
