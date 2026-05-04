//! JSON DRY-section builders: duplicates, dead_code, fragments,
//! wildcards, boilerplate, repeated_matches. Each typed `DryFinding`
//! variant projects to its corresponding JSON section type.

use std::collections::HashSet;

use super::super::json_types::{
    JsonBoilerplateFind, JsonDeadCodeWarning, JsonDuplicateEntry, JsonDuplicateGroup,
    JsonFragmentEntry, JsonFragmentGroup, JsonRepeatedMatchEntry, JsonRepeatedMatchGroup,
    JsonWildcardWarning,
};
use crate::domain::findings::{DryFinding, DryFindingDetails, DryFindingKind};

pub(super) fn build_duplicates(findings: &[DryFinding]) -> Vec<JsonDuplicateGroup> {
    let mut seen: HashSet<Vec<(String, usize)>> = HashSet::new();
    let mut groups = Vec::new();
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .for_each(|f| {
            if let DryFindingDetails::Duplicate { participants } = &f.details {
                let key: Vec<(String, usize)> = participants
                    .iter()
                    .map(|p| (p.file.clone(), p.line))
                    .collect();
                if !seen.insert(key) {
                    return;
                }
                groups.push(JsonDuplicateGroup {
                    kind: f.kind.meta().json_label.to_string(),
                    similarity: None,
                    entries: participants
                        .iter()
                        .map(|p| JsonDuplicateEntry {
                            name: p.function_name.clone(),
                            qualified_name: p.function_name.clone(),
                            file: p.file.clone(),
                            line: p.line,
                        })
                        .collect(),
                });
            }
        });
    groups
}

pub(super) fn build_dead_code(findings: &[DryFinding]) -> Vec<JsonDeadCodeWarning> {
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
            ) => Some(JsonDeadCodeWarning {
                function_name: qualified_name.clone(),
                qualified_name: qualified_name.clone(),
                file: f.common.file.clone(),
                line: f.common.line,
                kind: f.kind.meta().json_label.to_string(),
                suggestion: suggestion.clone().unwrap_or_default(),
            }),
            _ => None,
        })
        .collect()
}

pub(super) fn build_fragments(findings: &[DryFinding]) -> Vec<JsonFragmentGroup> {
    let mut seen: HashSet<Vec<(String, usize)>> = HashSet::new();
    let mut groups = Vec::new();
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .for_each(|f| {
            if let DryFindingDetails::Fragment {
                participants,
                statement_count,
            } = &f.details
            {
                let key: Vec<(String, usize)> = participants
                    .iter()
                    .map(|p| (p.file.clone(), p.line))
                    .collect();
                if !seen.insert(key) {
                    return;
                }
                groups.push(JsonFragmentGroup {
                    statement_count: *statement_count,
                    entries: participants
                        .iter()
                        .map(|p| JsonFragmentEntry {
                            function_name: p.function_name.clone(),
                            qualified_name: p.function_name.clone(),
                            file: p.file.clone(),
                            start_line: p.line,
                            end_line: p.end_line,
                        })
                        .collect(),
                });
            }
        });
    groups
}

pub(super) fn build_wildcards(findings: &[DryFinding]) -> Vec<JsonWildcardWarning> {
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .filter_map(|f| match &f.details {
            DryFindingDetails::Wildcard { module_path } => Some(JsonWildcardWarning {
                file: f.common.file.clone(),
                line: f.common.line,
                module_path: module_path.clone(),
            }),
            _ => None,
        })
        .collect()
}

pub(super) fn build_boilerplate(findings: &[DryFinding]) -> Vec<JsonBoilerplateFind> {
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .filter_map(|f| match &f.details {
            DryFindingDetails::Boilerplate {
                pattern_id,
                struct_name,
                suggestion,
            } => Some(JsonBoilerplateFind {
                pattern_id: pattern_id.clone(),
                file: f.common.file.clone(),
                line: f.common.line,
                struct_name: struct_name.clone(),
                description: f.common.message.clone(),
                suggestion: suggestion.clone(),
            }),
            _ => None,
        })
        .collect()
}

pub(super) fn build_repeated_matches(findings: &[DryFinding]) -> Vec<JsonRepeatedMatchGroup> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut groups = Vec::new();
    findings
        .iter()
        .filter(|f| !f.common.suppressed)
        .for_each(|f| {
            if let DryFindingDetails::RepeatedMatch {
                enum_name,
                participants,
            } = &f.details
            {
                if !seen.insert(enum_name.clone()) {
                    return;
                }
                groups.push(JsonRepeatedMatchGroup {
                    enum_name: enum_name.clone(),
                    entries: participants
                        .iter()
                        .map(|p| JsonRepeatedMatchEntry {
                            file: p.file.clone(),
                            line: p.line,
                            function_name: p.function_name.clone(),
                            arm_count: 0,
                        })
                        .collect(),
                });
            }
        });
    groups
}
