//! Adapter layer — concrete implementations of the ports.
//!
//! Each concrete thing rustqual needs to do (load sources, parse config,
//! run dimension-specific checks, emit reports) lives here as an adapter.
//! Application use-cases consume adapters through the port traits defined
//! in `crate::ports`, never via direct imports from this module.
//!
//! Sub-modules:
//! - `analyzers/` — one sub-module per analysis dimension (iosp, complexity,
//!   dry, srp, coupling, tq, structural, architecture).
//! - `shared/` — utilities used by multiple analyzer adapters
//!   (AST normalization, helper visitors).

// During the Phase 3 migration, most adapter call sites still come from
// the pipeline module via direct imports. The allow is lifted as each
// adapter's integration moves to the Application layer in Phase 5.
#![allow(dead_code, unused_imports)]

pub mod analyzers;
pub mod config;
pub mod report;
pub mod shared;
pub mod source;
pub mod suppression;
