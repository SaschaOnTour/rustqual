//! Pure domain concepts of rustqual.
//!
//! The `domain` module holds framework-free value types that describe the
//! language of code-quality analysis: dimensions, severities, suppressions.
//! No `syn`, no I/O, no external libraries beyond `serde` derives and
//! `thiserror` — these types are the shared vocabulary used by every layer.
//!
//! New Domain types land here as the Clean-Architecture refactor progresses.
//! Phase 1 introduces `Dimension`, `Severity`, `Suppression`.

pub mod analysis_data;
pub mod dimension;
pub mod finding;
pub mod findings;
pub mod score;
pub mod severity;
pub mod source_unit;
pub mod suppression;

pub use analysis_data::AnalysisData;
pub use dimension::Dimension;
pub use finding::Finding;
pub use findings::AnalysisFindings;
pub use score::PERCENTAGE_MULTIPLIER;
pub use severity::Severity;
pub use source_unit::SourceUnit;
pub use suppression::Suppression;

#[cfg(test)]
mod tests;
