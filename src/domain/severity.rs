//! Severity classification for findings.
//!
//! Severity is a coarse bucketing of how important a finding is. It is
//! derived from finding-specific metadata (e.g. total number of problematic
//! locations in an IOSP violation) by the analyzers; the Domain type itself
//! carries no classification logic.

use serde::Serialize;

/// Severity of a finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Minor finding; informational.
    Low,
    /// Notable finding; should be addressed.
    Medium,
    /// Serious finding; blocks merge under strict policies.
    High,
}
