pub mod boilerplate;
pub(crate) mod call_targets;
pub mod dead_code;
pub mod fragments;
pub mod functions;
pub mod match_patterns;
pub mod wildcards;

pub use boilerplate::BoilerplateFind;
pub use dead_code::{DeadCodeKind, DeadCodeWarning};
pub use fragments::FragmentGroup;
pub use functions::{DuplicateGroup, DuplicateKind};

use syn::visit::Visit;

use crate::adapters::shared::normalize::NormalizedToken;

// ── Shared visitor infrastructure ──────────────────────────────

/// Trait for AST visitors that need per-file state reset.
pub(crate) trait FileVisitor {
    fn reset_for_file(&mut self, file_path: &str);
}

/// Visit all parsed files with a visitor, resetting per-file state.
/// Trivial: iteration with trait method call.
pub(crate) fn visit_all_files<'a, V>(parsed: &'a [(String, String, syn::File)], visitor: &mut V)
where
    V: FileVisitor + Visit<'a>,
{
    parsed.iter().for_each(|(path, _, file)| {
        visitor.reset_for_file(path);
        syn::visit::visit_file(visitor, file);
    });
}

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

/// A declared function with metadata for dead code analysis.
pub struct DeclaredFunction {
    pub name: String,
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
    pub is_test: bool,
    pub is_main: bool,
    pub is_trait_impl: bool,
    pub has_allow_dead_code: bool,
    /// Whether this function is marked as public API via `// qual:api`.
    pub is_api: bool,
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

// ── Attribute helpers ───────────────────────────────────────────

// `has_cfg_test` and `has_test_attr` live in `adapters::shared::cfg_test`
// (multi-dimension utility). Re-exports keep existing call sites working.
pub(crate) use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};

/// Check if attributes contain `#[allow(dead_code)]`.
/// Operation: attribute inspection logic.
fn has_allow_dead_code(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("allow")
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|ident| ident == "dead_code")
    })
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
