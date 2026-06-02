use super::*;

// ── SDP suppression tests ──────────────────────────────

#[test]
fn test_count_sdp_violations_excludes_suppressed() {
    let analysis = crate::adapters::analyzers::coupling::CouplingAnalysis {
        metrics: vec![],
        cycles: vec![],
        sdp_violations: vec![
            crate::adapters::analyzers::coupling::sdp::SdpViolation {
                from_module: "a".into(),
                to_module: "b".into(),
                from_instability: 0.2,
                to_instability: 0.8,
                suppressed: true,
            },
            crate::adapters::analyzers::coupling::sdp::SdpViolation {
                from_module: "c".into(),
                to_module: "d".into(),
                from_instability: 0.3,
                to_instability: 0.9,
                suppressed: false,
            },
        ],
        graph: crate::adapters::analyzers::coupling::ModuleGraph::default(),
    };
    let config = crate::config::sections::CouplingConfig::default();
    let mut summary = Summary::from_results(&[]);
    count_sdp_violations(Some(&analysis), &config, &mut summary);
    assert_eq!(
        summary.sdp_violations, 1,
        "Only unsuppressed violations counted"
    );
}

#[test]
fn dry_suppression_marks_every_group_kind() {
    // `qual:allow(dry)` on the line before an entry (line 4 → entry line 5)
    // suppresses every DRY finding kind: duplicate, repeated-match, fragment,
    // and boilerplate.
    let sups = dry_suppression_at(4);
    let (mut dup, mut rep, mut frag, mut bp) = dry_group_fixtures();
    mark_dry_suppressions(std::slice::from_mut(&mut dup), &sups);
    mark_dry_suppressions(std::slice::from_mut(&mut rep), &sups);
    mark_dry_suppressions(std::slice::from_mut(&mut frag), &sups);
    mark_dry_suppressions(std::slice::from_mut(&mut bp), &sups);
    assert!(
        dup.suppressed,
        "duplicate group suppressed by qual:allow(dry)"
    );
    assert!(rep.suppressed, "repeated-match group suppressed");
    assert!(frag.suppressed, "fragment group suppressed");
    assert!(bp.suppressed, "boilerplate find suppressed");
}

#[test]
fn test_duplicate_without_suppression_not_marked() {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };

    let mut groups = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "foo".to_string(),
                qualified_name: "foo".to_string(),
                file: "test.rs".to_string(),
                line: 5,
            },
            DuplicateEntry {
                name: "bar".to_string(),
                qualified_name: "bar".to_string(),
                file: "test.rs".to_string(),
                line: 15,
            },
        ],
        kind: DuplicateKind::Exact,
        suppressed: false,
    }];

    let suppression_lines: std::collections::HashMap<String, Vec<Suppression>> =
        std::collections::HashMap::new();

    mark_dry_suppressions(&mut groups, &suppression_lines);
    assert!(
        !groups[0].suppressed,
        "Group without suppression should not be marked"
    );
}

#[test]
fn test_inverse_annotation_suppresses_duplicate() {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };

    let mut groups = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "as_str".to_string(),
                qualified_name: "Foo::as_str".to_string(),
                file: "test.rs".to_string(),
                line: 5,
            },
            DuplicateEntry {
                name: "parse".to_string(),
                qualified_name: "Foo::parse".to_string(),
                file: "test.rs".to_string(),
                line: 15,
            },
        ],
        kind: DuplicateKind::NearDuplicate { similarity: 0.91 },
        suppressed: false,
    }];

    // qual:inverse(parse) on line 4 (one before as_str at line 5)
    let inverse_lines: std::collections::HashMap<String, Vec<(usize, String)>> =
        [("test.rs".to_string(), vec![(4, "parse".to_string())])].into();

    mark_inverse_suppressions(&mut groups, &inverse_lines);
    assert!(
        groups[0].suppressed,
        "Inverse-annotated pair should be suppressed"
    );
}

#[test]
fn test_inverse_annotation_must_target_group_member() {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };

    let mut groups = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "foo".to_string(),
                qualified_name: "foo".to_string(),
                file: "test.rs".to_string(),
                line: 5,
            },
            DuplicateEntry {
                name: "bar".to_string(),
                qualified_name: "bar".to_string(),
                file: "test.rs".to_string(),
                line: 15,
            },
        ],
        kind: DuplicateKind::Exact,
        suppressed: false,
    }];

    // qual:inverse(baz) targets a function not in the group
    let inverse_lines: std::collections::HashMap<String, Vec<(usize, String)>> =
        [("test.rs".to_string(), vec![(4, "baz".to_string())])].into();

    mark_inverse_suppressions(&mut groups, &inverse_lines);
    assert!(
        !groups[0].suppressed,
        "Inverse targeting non-member should not suppress"
    );
}
