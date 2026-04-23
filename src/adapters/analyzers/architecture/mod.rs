//! Architecture-Dimension implementation.
//!
//! Hosts the Architecture analyzer — rule evaluation over syn ASTs. Four
//! rule families live here: the Layer Rule, the Forbidden Rule, the
//! Symbol-Pattern matcher family (seven matchers), and the Trait-
//! Signature Rule.

// Some internal helpers are exercised only through unit tests; scope
// the relaxed warnings to test builds so non-test compiles still
// surface dead-code or unused-import regressions.
#![cfg_attr(test, allow(dead_code, unused_imports))]

pub mod analyzer;
pub mod call_parity_rule;
pub mod compiled;
pub mod explain;
pub mod forbidden_rule;
pub mod layer_rule;
pub mod matcher;
pub(crate) mod rendering;
pub mod trait_contract_rule;
pub mod violation;

pub use analyzer::ArchitectureAnalyzer;

pub use violation::{MatchLocation, ViolationKind};

#[cfg(test)]
mod tests;
