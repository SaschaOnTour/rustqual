//! Cross-analyzer utilities.
//!
//! Things that multiple analyzer adapters need (AST normalization for
//! DRY and duplicate detection, common visitor helpers, shared types
//! outside the Domain) live here. Nothing in this module is allowed to
//! depend on a specific analyzer.
pub mod normalize;

#[cfg(test)]
mod tests;
