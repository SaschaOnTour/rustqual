//! Application layer — use-cases that orchestrate adapters through ports.
//!
//! The Application layer is rustqual's business-logic tier. Each use-case
//! is a small function that takes ports as parameters and returns domain
//! values. Use-cases know no `syn`, no I/O, and no concrete adapter
//! implementations — everything flows through the `crate::ports` traits
//! and the `crate::domain` value types.
//!
//! Phase 5 lands the first port-based use-case: [`analyze_codebase`],
//! which iterates over a slice of dimension analyzers and concatenates
//! their findings. The existing pipeline module continues to handle the
//! rich per-dimension output paths until later phases migrate each
//! dimension onto the same port.

pub mod analyze_codebase;
pub(crate) mod exit_gates;
pub(crate) mod setup;

pub use analyze_codebase::analyze_codebase;
pub(crate) use exit_gates::apply_exit_gates;
pub(crate) use setup::setup_config;
