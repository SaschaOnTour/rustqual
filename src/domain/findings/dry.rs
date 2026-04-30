//! Typed Finding for the DRY dimension.
//!
//! DRY is the most heterogeneous dimension: it produces six distinct
//! finding shapes (Duplicate, Fragment, DeadCode, Wildcard, Boilerplate,
//! RepeatedMatch). Each variant of `DryFindingDetails` carries its own
//! per-finding data; the wrapping `DryFinding` keeps a uniform surface
//! for collection and rendering.

use crate::domain::Finding;

/// Sub-category of DRY finding. Mirrors the rule-id last segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DryFindingKind {
    DuplicateExact,
    DuplicateSimilar,
    Fragment,
    DeadCodeUncalled,
    DeadCodeTestOnly,
    Wildcard,
    Boilerplate,
    RepeatedMatch,
}

/// Per-kind static labels used by reporters. Centralised so the
/// kind→string mapping happens in one place — adding a new kind
/// variant is a single-match update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DryKindMeta {
    /// HTML kind label (e.g. `"Exact"`, `"Similar"`).
    pub html_kind_label: &'static str,
    /// HTML dead-code tag (e.g. `"uncalled"`, `"test-only"`); empty
    /// for non-dead-code variants.
    pub html_dead_code_tag: &'static str,
    /// JSON kind label (e.g. `"exact"`, `"near_duplicate"`,
    /// `"uncalled"`, `"test_only"`).
    pub json_label: &'static str,
}

impl DryFindingKind {
    /// Static metadata for this kind: HTML labels, JSON labels.
    pub const fn meta(self) -> DryKindMeta {
        let (html, dead_tag, json) = match self {
            Self::DuplicateExact => ("Exact", "", "exact"),
            Self::DuplicateSimilar => ("Similar", "", "near_duplicate"),
            Self::Fragment => ("Fragment", "", "fragment"),
            Self::DeadCodeUncalled => ("Dead", "uncalled", "uncalled"),
            Self::DeadCodeTestOnly => ("Dead", "test-only", "test_only"),
            Self::Wildcard => ("Wildcard", "", "wildcard"),
            Self::Boilerplate => ("Boilerplate", "", "boilerplate"),
            Self::RepeatedMatch => ("Repeated", "", "repeated_match"),
        };
        DryKindMeta {
            html_kind_label: html,
            html_dead_code_tag: dead_tag,
            json_label: json,
        }
    }
}

/// One participant in a duplicate-function group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateParticipant {
    pub function_name: String,
    pub file: String,
    pub line: usize,
}

/// One participant in a fragment group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FragmentParticipant {
    pub function_name: String,
    pub file: String,
    pub line: usize,
}

/// One participant in a repeated-match group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatedMatchParticipant {
    pub function_name: String,
    pub file: String,
    pub line: usize,
}

/// Per-variant detail for a DRY finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DryFindingDetails {
    Duplicate {
        participants: Vec<DuplicateParticipant>,
    },
    Fragment {
        participants: Vec<FragmentParticipant>,
        statement_count: usize,
    },
    DeadCode {
        qualified_name: String,
        suggestion: Option<String>,
    },
    Wildcard {
        module_path: String,
    },
    Boilerplate {
        pattern_id: String,
        struct_name: Option<String>,
        suggestion: String,
    },
    RepeatedMatch {
        enum_name: String,
        participants: Vec<RepeatedMatchParticipant>,
    },
}

/// DRY finding — duplicate code, dead code, wildcard import, etc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryFinding {
    /// Common metadata. `common.dimension == Dimension::Dry`.
    pub common: Finding,
    /// Which DRY sub-category triggered.
    pub kind: DryFindingKind,
    /// Per-variant detail.
    pub details: DryFindingDetails,
}
