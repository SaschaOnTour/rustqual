// qual:allow(coupling) reason: "application layer orchestrates adapters + ports — high instability is expected"
//! Application layer — use-cases that orchestrate adapters through ports.
//!
//! The Application layer is rustqual's business-logic tier. Each use-case
//! is a small function that takes ports as parameters and returns domain
//! values. Use-cases know no `syn`, no I/O, and no concrete adapter
//! implementations — everything flows through the `crate::ports` traits
//! and the `crate::domain` value types.

pub mod analyze_codebase;
mod architecture;
pub(crate) mod dry_suppressions;
pub(crate) mod exit_gates;
pub(crate) mod metrics;
pub(crate) mod orphan_suppressions;
pub(crate) mod pipeline;
pub(crate) mod projection;
pub(crate) mod secondary;
pub(crate) mod setup;
pub(crate) mod structural_metrics;
pub(crate) mod suppression_windows;
pub(crate) mod tq_metrics;
pub(crate) mod warnings;

pub use analyze_codebase::analyze_codebase;
pub(crate) use exit_gates::apply_exit_gates;
pub(crate) use pipeline::{analyze_and_output, output_results, run_analysis};
pub(crate) use setup::setup_config;

#[cfg(test)]
mod tests;
