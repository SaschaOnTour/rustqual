//! Shared DRY projection: split `&[DryFinding]` into typed buckets
//! (duplicate-groups, fragment-groups, repeated-match-groups, dead
//! code, boilerplate, wildcards). Group-style buckets are deduped by
//! participant-location set.

use crate::adapters::report::dry_dedup::dedup_by_key;
use crate::domain::findings::{
    DryFinding, DryFindingDetails, DryFindingKind, DuplicateParticipant, FragmentParticipant,
    RepeatedMatchParticipant,
};

/// Atomic participant row, shared across reporters and across the
/// three group-style sub-categories (duplicates, fragments, repeated
/// matches).
pub(crate) struct ParticipantRow {
    pub function_name: String,
    pub file: String,
    pub line: usize,
}

/// Atomic group with a kind/label header + participant list.
pub(crate) struct DryGroupRow {
    /// Header text — for duplicates this is `DryFindingKind::meta().html_kind_label`
    /// ("Exact"/"Similar"); for fragments it's the formatted statement
    /// count ("3 stmts"); for repeated-match it's the enum name. Owned
    /// String because it can be dynamic.
    pub kind_label: String,
    pub participants: Vec<ParticipantRow>,
}

/// Atomic dead-code row.
pub(crate) struct DeadCodeRow {
    pub qualified_name: String,
    pub kind_tag: &'static str,
    pub file: String,
    pub line: usize,
    pub suggestion: String,
}

/// Atomic boilerplate row.
pub(crate) struct BoilerplateRow {
    pub pattern_id: String,
    /// Empty string when finding has no struct context (rendered as
    /// "(anonymous)" by formatters).
    pub struct_name: String,
    pub file: String,
    pub line: usize,
    pub message: String,
    pub suggestion: String,
}

/// Atomic wildcard row.
pub(crate) struct WildcardRow {
    pub module_path: String,
    pub file: String,
    pub line: usize,
}

/// The DRY buckets, reporter-agnostic. Dead functions and dead types share
/// `dead_code`: both answer "nothing refers to this", and the human-facing
/// table tells them apart by the kind tag.
pub(crate) struct DryBuckets {
    pub duplicate_groups: Vec<DryGroupRow>,
    pub fragment_groups: Vec<DryGroupRow>,
    pub repeated_match_groups: Vec<DryGroupRow>,
    pub dead_code: Vec<DeadCodeRow>,
    pub boilerplate: Vec<BoilerplateRow>,
    pub wildcards: Vec<WildcardRow>,
}

/// Project DRY findings into the typed buckets. Group-style
/// buckets are deduped by participant-location set so the same group
/// only appears once even if the analyzer emitted one finding per
/// participant.
pub(crate) fn split_dry_findings(findings: &[DryFinding]) -> DryBuckets {
    DryBuckets {
        duplicate_groups: build_duplicate_groups(findings),
        fragment_groups: build_fragment_groups(findings),
        repeated_match_groups: build_repeated_match_groups(findings),
        dead_code: build_dead_code(findings),
        boilerplate: build_boilerplate(findings),
        wildcards: build_wildcards(findings),
    }
}

fn dup_participants(p: &[DuplicateParticipant]) -> Vec<ParticipantRow> {
    p.iter()
        .map(|p| ParticipantRow {
            function_name: p.function_name.clone(),
            file: p.file.clone(),
            line: p.line,
        })
        .collect()
}

fn frag_participants(p: &[FragmentParticipant]) -> Vec<ParticipantRow> {
    p.iter()
        .map(|p| ParticipantRow {
            function_name: p.function_name.clone(),
            file: p.file.clone(),
            line: p.line,
        })
        .collect()
}

fn rep_participants(p: &[RepeatedMatchParticipant]) -> Vec<ParticipantRow> {
    p.iter()
        .map(|p| ParticipantRow {
            function_name: p.function_name.clone(),
            file: p.file.clone(),
            line: p.line,
        })
        .collect()
}

fn build_duplicate_groups(findings: &[DryFinding]) -> Vec<DryGroupRow> {
    dedup_by_key(findings, |f| match (&f.kind, &f.details) {
        (
            DryFindingKind::DuplicateExact | DryFindingKind::DuplicateSimilar,
            DryFindingDetails::Duplicate { participants, .. },
        ) => {
            let key: Vec<(String, usize)> = participants
                .iter()
                .map(|p| (p.file.clone(), p.line))
                .collect();
            Some((
                DryGroupRow {
                    kind_label: f.kind.meta().html_kind_label.to_string(),
                    participants: dup_participants(participants),
                },
                key,
            ))
        }
        _ => None,
    })
}

fn build_fragment_groups(findings: &[DryFinding]) -> Vec<DryGroupRow> {
    dedup_by_key(findings, |f| match &f.details {
        DryFindingDetails::Fragment {
            participants,
            statement_count,
        } => {
            let key: Vec<(String, usize)> = participants
                .iter()
                .map(|p| (p.file.clone(), p.line))
                .collect();
            Some((
                DryGroupRow {
                    kind_label: format!("{statement_count} stmts"),
                    participants: frag_participants(participants),
                },
                key,
            ))
        }
        _ => None,
    })
}

fn build_repeated_match_groups(findings: &[DryFinding]) -> Vec<DryGroupRow> {
    dedup_by_key(findings, |f| match &f.details {
        DryFindingDetails::RepeatedMatch {
            enum_name,
            participants,
        } => {
            // Mirror JSON's grouping: `(enum_name, sorted locations)`.
            // Without `enum_name` two distinct repeated patterns over
            // the same participant set collapse into one group; without
            // sorting, the same group emitted with participants in
            // different order would dedupe as separate groups.
            let mut locations: Vec<(String, usize)> = participants
                .iter()
                .map(|p| (p.file.clone(), p.line))
                .collect();
            locations.sort();
            Some((
                DryGroupRow {
                    kind_label: enum_name.clone(),
                    participants: rep_participants(participants),
                },
                (enum_name.clone(), locations),
            ))
        }
        _ => None,
    })
}

fn build_dead_code(findings: &[DryFinding]) -> Vec<DeadCodeRow> {
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .filter_map(dead_row)
        .collect()
}

/// A dead function and a dead type answer the same question — "nothing refers
/// to this" — so the human-facing table lists them together, told apart by the
/// kind tag. Only where the name lives differs.
/// Operation: shape dispatch + row construction, no own calls.
fn dead_row(f: &DryFinding) -> Option<DeadCodeRow> {
    let (name, suggestion) = match &f.details {
        DryFindingDetails::DeadCode {
            qualified_name,
            suggestion,
        } => (qualified_name, suggestion.clone().unwrap_or_default()),
        DryFindingDetails::DeadType {
            name, suggestion, ..
        } => (name, suggestion.clone()),
        _ => return None,
    };
    Some(DeadCodeRow {
        qualified_name: name.clone(),
        kind_tag: f.kind.meta().html_dead_code_tag,
        file: f.common.file.clone(),
        line: f.common.line,
        suggestion,
    })
}

fn build_boilerplate(findings: &[DryFinding]) -> Vec<BoilerplateRow> {
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .filter_map(|f| match &f.details {
            DryFindingDetails::Boilerplate {
                pattern_id,
                struct_name,
                suggestion,
            } => Some(BoilerplateRow {
                pattern_id: pattern_id.clone(),
                struct_name: struct_name.clone().unwrap_or_default(),
                file: f.common.file.clone(),
                line: f.common.line,
                message: f.common.message.clone(),
                suggestion: suggestion.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn build_wildcards(findings: &[DryFinding]) -> Vec<WildcardRow> {
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .filter_map(|f| match &f.details {
            DryFindingDetails::Wildcard { module_path } => Some(WildcardRow {
                module_path: module_path.clone(),
                file: f.common.file.clone(),
                line: f.common.line,
            }),
            _ => None,
        })
        .collect()
}
