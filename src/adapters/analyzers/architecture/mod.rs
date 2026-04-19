//! Architecture-Dimension implementation.
//!
//! This module hosts the Architecture analyzer — rule evaluation over syn ASTs.
//! Phase 1 delivers the first two matchers (`forbid_path_prefix`, `forbid_glob_import`);
//! subsequent phases extend the matcher set, the layer rule, forbidden edges and
//! trait-signature rules.
//!
//! Placement note: this module lives at the crate root in Phase 1 and moves into
//! `src/adapters/analyzers/architecture/` in Phase 3, when all analyzers are
//! migrated to the adapter layer. The move is mechanical — nothing here
//! references specific outer paths.

// Types in this module are exercised by the Phase-1 matcher tests but not yet
// consumed by a pipeline/adapter (that integration lands in Phase 3). Suppress
// dead-code and unused-import warnings at the module boundary — individual
// items are verified through unit tests.
#![allow(dead_code, unused_imports)]

pub mod analyzer;
pub mod cli;
pub mod compiled;
pub mod explain;
pub mod forbidden_rule;
pub mod layer_rule;
pub mod matcher;
pub(crate) mod use_tree;
pub mod violation;

pub use analyzer::ArchitectureAnalyzer;

pub use violation::{MatchLocation, ViolationKind};

#[cfg(test)]
mod tests;
