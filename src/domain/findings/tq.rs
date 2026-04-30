//! Typed Finding for the Test-Quality dimension.
//!
//! TQ findings are uniform in shape — `kind` selects which of the five
//! TQ checks fired (TQ-001..TQ-005). Coverage-data findings (TQ-005)
//! optionally carry a list of uncovered (file, line) pairs.

use crate::domain::Finding;

/// Sub-category of Test-Quality finding (mirrors TQ-001 through TQ-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TqFindingKind {
    NoAssertion,
    NoSut,
    Untested,
    Uncovered,
    UntestedLogic,
}

/// Per-kind static labels used by reporters. Centralised here so the
/// kind→string mapping happens in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TqKindMeta {
    pub ai_category: &'static str,
    pub findings_list_category: &'static str,
    pub sarif_rule: &'static str,
    /// Short snake-case JSON kind label (e.g. `"no_assertion"`).
    pub json_kind: &'static str,
    /// Human-readable label with rule id (e.g. `"TQ-001 No assertion"`).
    pub display_label: &'static str,
}

impl TqFindingKind {
    /// Static metadata for this kind: AI category, findings_list category, SARIF rule id.
    pub const fn meta(self) -> TqKindMeta {
        let (ai, fl, sarif, json, display) = match self {
            Self::NoAssertion => (
                "no_assertion",
                "TQ_NO_ASSERT",
                "TQ-001",
                "no_assertion",
                "TQ-001 No assertion",
            ),
            Self::NoSut => (
                "no_sut_call",
                "TQ_NO_SUT",
                "TQ-002",
                "no_sut",
                "TQ-002 No SUT call",
            ),
            Self::Untested => (
                "untested",
                "TQ_UNTESTED",
                "TQ-003",
                "untested",
                "TQ-003 Untested",
            ),
            Self::Uncovered => (
                "uncovered",
                "TQ_UNCOVERED",
                "TQ-004",
                "uncovered",
                "TQ-004 Uncovered",
            ),
            Self::UntestedLogic => (
                "untested_logic",
                "TQ_UNTESTED_LOGIC",
                "TQ-005",
                "untested_logic",
                "TQ-005 Untested logic",
            ),
        };
        TqKindMeta {
            ai_category: ai,
            findings_list_category: fl,
            sarif_rule: sarif,
            json_kind: json,
            display_label: display,
        }
    }
}

/// Test-Quality finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TqFinding {
    /// Common metadata. `common.dimension == Dimension::TestQuality`.
    pub common: Finding,
    /// Which TQ check fired.
    pub kind: TqFindingKind,
    /// Function name being tested or untested.
    pub function_name: String,
    /// Optional uncovered-line pairs for TQ-005 (untested logic).
    pub uncovered_lines: Option<Vec<(String, usize)>>,
}
