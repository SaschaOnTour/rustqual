//! Shared DRY projection: split `&[DryFinding]` into typed buckets
//! (duplicate-groups, fragment-groups, repeated-match-groups, dead
//! code, boilerplate, wildcards). Group-style buckets are deduped by
//! participant-location set.

use crate::adapters::report::dry_dedup::dedup_by_locations;
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

/// All six DRY buckets, reporter-agnostic.
pub(crate) struct DryBuckets {
    pub duplicate_groups: Vec<DryGroupRow>,
    pub fragment_groups: Vec<DryGroupRow>,
    pub repeated_match_groups: Vec<DryGroupRow>,
    pub dead_code: Vec<DeadCodeRow>,
    pub boilerplate: Vec<BoilerplateRow>,
    pub wildcards: Vec<WildcardRow>,
}

/// Project DRY findings into the six typed buckets. Group-style
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
    dedup_by_locations(findings, |f| match (&f.kind, &f.details) {
        (
            DryFindingKind::DuplicateExact | DryFindingKind::DuplicateSimilar,
            DryFindingDetails::Duplicate { participants },
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
    dedup_by_locations(findings, |f| match &f.details {
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
    use std::collections::HashSet;
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .for_each(|f| {
            if let DryFindingDetails::RepeatedMatch {
                enum_name,
                participants,
            } = &f.details
            {
                if seen.insert(enum_name.clone()) {
                    out.push(DryGroupRow {
                        kind_label: enum_name.clone(),
                        participants: rep_participants(participants),
                    });
                }
            }
        });
    out
}

fn build_dead_code(findings: &[DryFinding]) -> Vec<DeadCodeRow> {
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .filter_map(|f| match (&f.kind, &f.details) {
            (
                DryFindingKind::DeadCodeUncalled | DryFindingKind::DeadCodeTestOnly,
                DryFindingDetails::DeadCode {
                    qualified_name,
                    suggestion,
                },
            ) => Some(DeadCodeRow {
                qualified_name: qualified_name.clone(),
                kind_tag: f.kind.meta().html_dead_code_tag,
                file: f.common.file.clone(),
                line: f.common.line,
                suggestion: suggestion.clone().unwrap_or_default(),
            }),
            _ => None,
        })
        .collect()
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
