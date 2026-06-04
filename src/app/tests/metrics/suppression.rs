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
fn dry_suppression_must_cover_the_dry_dimension() {
    // A marker in the line window but covering a *different* dimension must not
    // suppress a DRY group. Pins the `is_within_window && covers(dry)` filter
    // against `&&`→`||` (which would suppress on window alone).
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };
    let mut groups = vec![DuplicateGroup {
        entries: vec![DuplicateEntry {
            name: "foo".into(),
            qualified_name: "foo".into(),
            file: "test.rs".into(),
            line: 5,
        }],
        kind: DuplicateKind::Exact,
        suppressed: false,
    }];
    // Marker on line 4 (in window of entry line 5) but covering Complexity.
    let sups: std::collections::HashMap<String, Vec<Suppression>> = [(
        "test.rs".to_string(),
        vec![Suppression {
            line: 4,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    )]
    .into();
    mark_dry_suppressions(&mut groups, &sups);
    assert!(
        !groups[0].suppressed,
        "an in-window marker for another dimension must not suppress DRY"
    );
}

/// Build a single near-duplicate group `[as_str@as_line, parse@15]` and mark it
/// against a `qual:inverse(parse)` annotation at `inv_line`.
fn inverse_suppressed(inv_line: usize, as_line: usize) -> bool {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };
    let mut groups = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "as_str".into(),
                qualified_name: "Foo::as_str".into(),
                file: "test.rs".into(),
                line: as_line,
            },
            DuplicateEntry {
                name: "parse".into(),
                qualified_name: "Foo::parse".into(),
                file: "test.rs".into(),
                line: 15,
            },
        ],
        kind: DuplicateKind::NearDuplicate { similarity: 0.91 },
        suppressed: false,
    }];
    let inverse_lines: std::collections::HashMap<String, Vec<(usize, String)>> =
        [("test.rs".to_string(), vec![(inv_line, "parse".to_string())])].into();
    mark_inverse_suppressions(&mut groups, &inverse_lines);
    groups[0].suppressed
}

#[test]
fn inverse_suppression_window_boundaries() {
    // The inverse window is `line <= entry.line && entry.line - line <= WINDOW`
    // (WINDOW = 3). Annotation at line 1:
    //  - entry at line 4 → diff exactly 3 → suppressed (pins `<=`→`>` and the
    //    `entry.line - line` subtraction against `-`→`/`: 4/1=4 > 3).
    //  - entry at line 10 → diff 9 → not suppressed (pins `&&`→`||`, which would
    //    suppress on the `line <= entry.line` half alone).
    assert!(
        inverse_suppressed(1, 4),
        "inverse pair at the window edge (diff 3) is suppressed"
    );
    assert!(
        !inverse_suppressed(1, 10),
        "inverse pair outside the window (diff 9) is not suppressed"
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
