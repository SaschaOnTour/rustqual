//! Pure domain concepts of rustqual.
//!
//! The `domain` module holds framework-free value types that describe the
//! language of code-quality analysis: dimensions, severities, suppressions.
//! No `syn`, no I/O, no external libraries beyond `serde` derives and
//! `thiserror` — these types are the shared vocabulary used by every layer.
//!
//! New Domain types land here as the Clean-Architecture refactor progresses.
//! Phase 1 introduces `Dimension`, `Severity`, `Suppression`.

pub mod dimension;
pub mod severity;
pub mod suppression;

pub use dimension::Dimension;
pub use severity::Severity;
pub use suppression::Suppression;

#[cfg(test)]
mod tests;
