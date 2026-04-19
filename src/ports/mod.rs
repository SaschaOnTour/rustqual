//! Interfaces between the Application layer and Adapter implementations.
//!
//! Ports are the contract rustqual's use-cases program against. Each port
//! is a trait with typed error-enums and (where meaningful) object-safe
//! dispatch so that implementations can be swapped — real filesystem vs.
//! fake in tests, TOML config vs. environment variables, and so on.
//!
//! Ports in this module:
//! - [`source_loader::SourceLoader`] — yields `SourceUnit`s from a root path
//! - [`suppression_parser::SuppressionParser`] — extracts `Suppression`s
//! - [`reporter::Reporter`] — emits findings as human or machine output
//! - [`dimension_analyzer::DimensionAnalyzer`] — produces `Finding`s for one
//!   dimension (added in Phase 5 alongside the Application layer).

// Port items are defined here but not yet consumed by the use-case layer;
// the Application use-cases that wire them up arrive in Phase 5. Allow
// dead code and unused imports until then.
#![allow(dead_code, unused_imports)]

pub mod dimension_analyzer;
pub mod reporter;
pub mod source_loader;
pub mod suppression_parser;

pub use dimension_analyzer::{AnalysisContext, DimensionAnalyzer, ParsedFile};
pub use reporter::{ReportError, ReportPayload, Reporter};
pub use source_loader::{LoadError, SourceLoader};
pub use suppression_parser::{SuppressionParseError, SuppressionParser};

#[cfg(test)]
mod tests;
