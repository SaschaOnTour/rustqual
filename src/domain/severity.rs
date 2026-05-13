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

/// Per-severity reporter-format labels. SARIF uses `note/warning/error`,
/// GitHub Actions uses `notice/warning/error`, JSON uses lowercase
/// `low/medium/high`. Centralised here so the mapping is a single
/// source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeverityLevels {
    pub sarif: &'static str,
    pub github: &'static str,
    pub lowercase: &'static str,
}

impl Severity {
    /// Reporter-specific level labels for this severity.
    pub const fn levels(&self) -> SeverityLevels {
        let (sarif, github, lowercase) = match self {
            Self::Low => ("note", "notice", "low"),
            Self::Medium => ("warning", "warning", "medium"),
            Self::High => ("error", "error", "high"),
        };
        SeverityLevels {
            sarif,
            github,
            lowercase,
        }
    }
}
