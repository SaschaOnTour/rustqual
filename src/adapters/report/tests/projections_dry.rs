//! Tests for the shared DRY projection (`split_dry_findings`) — the
//! reporter-agnostic step that text and HTML reporters consume.
//!
//! Round 15 P2 (Codex): `build_repeated_match_groups` deduped by
//! `enum_name` alone, collapsing two distinct repeated-match groups
//! over the same enum into one bucket entry. The JSON reporter
//! builds its own groups (with the correct
//! `(enum_name, sorted locations)` key, see round 13), but text and
//! HTML went through the shared projection and silently lost the
//! second group. Reporter parity regressed once more. These tests
//! lock the dedup contract at the projection layer so every consumer
//! benefits.

use crate::adapters::report::projections::dry::split_dry_findings;
use crate::domain::findings::{
    DryFinding, DryFindingDetails, DryFindingKind, RepeatedMatchParticipant,
};
use crate::domain::{Dimension, Finding, Severity};

fn dry_common(line: usize) -> Finding {
    Finding {
        file: "lib.rs".into(),
        line,
        dimension: Dimension::Dry,
        rule_id: "dry/repeated_match".into(),
        message: "repeated match".into(),
        severity: Severity::Medium,
        ..Default::default()
    }
}

fn participant(name: &str, line: usize, arms: usize) -> RepeatedMatchParticipant {
    RepeatedMatchParticipant {
        function_name: name.into(),
        file: "lib.rs".into(),
        line,
        arm_count: arms,
    }
}

fn repeated_match(line: usize, participants: Vec<RepeatedMatchParticipant>) -> DryFinding {
    DryFinding {
        common: dry_common(line),
        kind: DryFindingKind::RepeatedMatch,
        details: DryFindingDetails::RepeatedMatch {
            enum_name: "MyEnum".into(),
            participants,
        },
    }
}

#[test]
fn split_dry_findings_keeps_distinct_repeated_match_groups_over_same_enum() {
    let group_a = vec![participant("fa1", 10, 4), participant("fa2", 20, 4)];
    let group_b = vec![participant("fb1", 50, 3), participant("fb2", 60, 3)];
    let findings = vec![repeated_match(10, group_a), repeated_match(50, group_b)];

    let buckets = split_dry_findings(&findings);

    assert_eq!(
        buckets.repeated_match_groups.len(),
        2,
        "two distinct repeated-match groups over the same enum must NOT \
         collapse via enum-name-only dedup; got {} groups",
        buckets.repeated_match_groups.len(),
    );
    let participant_lines: Vec<usize> = buckets
        .repeated_match_groups
        .iter()
        .flat_map(|g| g.participants.iter().map(|p| p.line))
        .collect();
    assert!(
        participant_lines.contains(&10) && participant_lines.contains(&50),
        "both groups' participants must survive projection (expected \
         lines 10 and 50); got {participant_lines:?}",
    );
}

#[test]
fn split_dry_findings_collapses_duplicate_repeated_match_group_emissions() {
    // The analyzer emits one finding per participant for repeated-match
    // groups too; dedup by (enum_name, sorted locations) must collapse
    // those into a single bucket entry.
    let group = vec![participant("fa1", 10, 4), participant("fa2", 20, 4)];
    let findings = vec![repeated_match(10, group.clone()), repeated_match(20, group)];

    let buckets = split_dry_findings(&findings);

    assert_eq!(
        buckets.repeated_match_groups.len(),
        1,
        "two findings describing the same participant set must collapse \
         to one bucket entry; got {} groups",
        buckets.repeated_match_groups.len(),
    );
}
