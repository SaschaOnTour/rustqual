//! Architecture-Dimension implementation.
//!
//! Hosts the Architecture analyzer — rule evaluation over syn ASTs. Four
//! rule families live here: the Layer Rule, the Forbidden Rule, the
//! Symbol-Pattern matcher family (seven matchers), and the Trait-
//! Signature Rule.

// Some internal helpers are exercised only through unit tests; leaving
// their visibility permissive keeps the test tree simple.
#![allow(dead_code, unused_imports)]

pub mod analyzer;
pub mod compiled;
pub mod explain;
pub mod forbidden_rule;
pub mod layer_rule;
pub mod matcher;
pub mod trait_contract_rule;
pub mod violation;

pub use analyzer::ArchitectureAnalyzer;

pub use violation::{MatchLocation, ViolationKind};

#[cfg(test)]
mod tests;
