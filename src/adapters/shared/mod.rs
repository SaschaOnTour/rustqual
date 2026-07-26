//! Cross-analyzer utilities.
//!
//! Things that multiple analyzer adapters need (AST normalization for
//! DRY and duplicate detection, common visitor helpers, shared types
//! outside the Domain) live here. Nothing in this module is allowed to
//! depend on a specific analyzer.
pub mod cfg_test;
pub mod cfg_test_files;
pub(crate) mod child_paths;
pub(crate) mod crate_roots;
pub mod declared_function;
pub mod declared_type;
pub mod file_to_module;
pub mod file_visitor;
pub mod macro_expansion;
pub mod macro_tokens;
pub(crate) mod marked_declaration;
pub mod normalize;
pub mod project_scope;
pub(crate) mod reachability;
pub mod test_references;
pub mod use_tree;

#[cfg(test)]
mod tests;
