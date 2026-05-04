//! DRY-dimension projection: 6 sub-categories (duplicate, fragment,
//! dead-code, wildcard, boilerplate, repeated-match) → typed
//! `Vec<DryFinding>`.

use crate::adapters::analyzers::dry::boilerplate::BoilerplateFind;
use crate::adapters::analyzers::dry::dead_code::{DeadCodeKind, DeadCodeWarning};
use crate::adapters::analyzers::dry::fragments::FragmentGroup;
use crate::adapters::analyzers::dry::functions::{DuplicateGroup, DuplicateKind};
use crate::adapters::analyzers::dry::match_patterns::RepeatedMatchGroup;
use crate::adapters::analyzers::dry::wildcards::WildcardImportWarning;
use crate::app::secondary::SecondaryResults;
use crate::domain::findings::{
    DryFinding, DryFindingDetails, DryFindingKind, DuplicateParticipant, FragmentParticipant,
    RepeatedMatchParticipant,
};
use crate::domain::{Dimension, Finding, Severity};

const DIM: Dimension = Dimension::Dry;
const SEV: Severity = Severity::Medium;

/// Project all DRY analyzer outputs into typed DryFinding entries.
///
/// The 6 DRY sub-categories are flattened into a single Vec where the
/// variant lives in `details` and the rule-id discriminator lives in
/// `common.rule_id`.
pub(crate) fn project_dry(secondary: &SecondaryResults) -> Vec<DryFinding> {
    let mut out = Vec::new();
    out.extend(
        secondary
            .duplicates
            .iter()
            .flat_map(project_duplicate_group),
    );
    out.extend(secondary.fragments.iter().flat_map(project_fragment_group));
    out.extend(secondary.dead_code.iter().map(project_dead_code));
    out.extend(secondary.wildcard_warnings.iter().map(project_wildcard));
    out.extend(secondary.boilerplate.iter().map(project_boilerplate));
    out.extend(
        secondary
            .repeated_matches
            .iter()
            .flat_map(project_repeated_match_group),
    );
    out
}

fn project_duplicate_group(group: &DuplicateGroup) -> Vec<DryFinding> {
    let (rule_id, kind) = match &group.kind {
        DuplicateKind::Exact => ("dry/duplicate/exact", DryFindingKind::DuplicateExact),
        DuplicateKind::NearDuplicate { .. } => {
            ("dry/duplicate/similar", DryFindingKind::DuplicateSimilar)
        }
    };
    let participants: Vec<DuplicateParticipant> = group
        .entries
        .iter()
        .map(|e| DuplicateParticipant {
            function_name: e.qualified_name.clone(),
            file: e.file.clone(),
            line: e.line,
        })
        .collect();
    group
        .entries
        .iter()
        .map(|e| DryFinding {
            common: Finding {
                file: e.file.clone(),
                line: e.line,
                column: 0,
                dimension: DIM,
                rule_id: rule_id.into(),
                message: format!("duplicate of {} other function(s)", participants.len() - 1),
                severity: SEV,
                suppressed: group.suppressed,
            },
            kind,
            details: DryFindingDetails::Duplicate {
                participants: participants.clone(),
            },
        })
        .collect()
}

fn project_fragment_group(group: &FragmentGroup) -> Vec<DryFinding> {
    let participants: Vec<FragmentParticipant> = group
        .entries
        .iter()
        .map(|e| FragmentParticipant {
            function_name: e.qualified_name.clone(),
            file: e.file.clone(),
            line: e.start_line,
            end_line: e.end_line,
        })
        .collect();
    group
        .entries
        .iter()
        .map(|e| DryFinding {
            common: Finding {
                file: e.file.clone(),
                line: e.start_line,
                column: 0,
                dimension: DIM,
                rule_id: "dry/fragment".into(),
                message: format!(
                    "duplicate {}-statement fragment shared with {} other location(s)",
                    group.statement_count,
                    participants.len() - 1
                ),
                severity: SEV,
                suppressed: group.suppressed,
            },
            kind: DryFindingKind::Fragment,
            details: DryFindingDetails::Fragment {
                participants: participants.clone(),
                statement_count: group.statement_count,
            },
        })
        .collect()
}

fn project_dead_code(warning: &DeadCodeWarning) -> DryFinding {
    let (rule_id, kind) = match warning.kind {
        DeadCodeKind::Uncalled => ("dry/dead_code/uncalled", DryFindingKind::DeadCodeUncalled),
        DeadCodeKind::TestOnly => ("dry/dead_code/testonly", DryFindingKind::DeadCodeTestOnly),
    };
    DryFinding {
        common: Finding {
            file: warning.file.clone(),
            line: warning.line,
            column: 0,
            dimension: DIM,
            rule_id: rule_id.into(),
            message: format!("dead code: {}", warning.qualified_name),
            severity: SEV,
            suppressed: false,
        },
        kind,
        details: DryFindingDetails::DeadCode {
            qualified_name: warning.qualified_name.clone(),
            suggestion: Some(warning.suggestion.clone()),
        },
    }
}

fn project_wildcard(warning: &WildcardImportWarning) -> DryFinding {
    DryFinding {
        common: Finding {
            file: warning.file.clone(),
            line: warning.line,
            column: 0,
            dimension: DIM,
            rule_id: "dry/wildcard".into(),
            message: format!("wildcard import: {}", warning.module_path),
            severity: SEV,
            suppressed: warning.suppressed,
        },
        kind: DryFindingKind::Wildcard,
        details: DryFindingDetails::Wildcard {
            module_path: warning.module_path.clone(),
        },
    }
}

fn project_boilerplate(find: &BoilerplateFind) -> DryFinding {
    DryFinding {
        common: Finding {
            file: find.file.clone(),
            line: find.line,
            column: 0,
            dimension: DIM,
            rule_id: format!("dry/boilerplate/{}", find.pattern_id),
            message: find.description.clone(),
            severity: SEV,
            suppressed: find.suppressed,
        },
        kind: DryFindingKind::Boilerplate,
        details: DryFindingDetails::Boilerplate {
            pattern_id: find.pattern_id.clone(),
            struct_name: find.struct_name.clone(),
            suggestion: find.suggestion.clone(),
        },
    }
}

fn project_repeated_match_group(group: &RepeatedMatchGroup) -> Vec<DryFinding> {
    let participants: Vec<RepeatedMatchParticipant> = group
        .entries
        .iter()
        .map(|e| RepeatedMatchParticipant {
            function_name: e.function_name.clone(),
            file: e.file.clone(),
            line: e.line,
        })
        .collect();
    group
        .entries
        .iter()
        .map(|e| DryFinding {
            common: Finding {
                file: e.file.clone(),
                line: e.line,
                column: 0,
                dimension: DIM,
                rule_id: "dry/repeated_match".into(),
                message: format!(
                    "repeated match on {} ({} occurrences)",
                    group.enum_name,
                    group.entries.len()
                ),
                severity: SEV,
                suppressed: group.suppressed,
            },
            kind: DryFindingKind::RepeatedMatch,
            details: DryFindingDetails::RepeatedMatch {
                enum_name: group.enum_name.clone(),
                participants: participants.clone(),
            },
        })
        .collect()
}
