//! Rendering tests for the text DRY section. A single-populated-category view
//! both renders that category's content (pinning each `push_*` against a no-op)
//! and flips the all-empty early-return guard (pinning each `&&` → `||`).
use crate::adapters::report::projections::dry::{
    BoilerplateRow, DeadCodeRow, DryGroupRow, ParticipantRow, WildcardRow,
};
use crate::report::text::dry::format_dry_section;
use crate::report::text::views::DryView;

fn empty_view() -> DryView {
    DryView {
        duplicate_groups: vec![],
        fragment_groups: vec![],
        dead_code: vec![],
        boilerplate: vec![],
        wildcards: vec![],
        repeated_match_groups: vec![],
    }
}

fn group(kind: &str, fname: &str, file: &str, line: usize) -> DryGroupRow {
    DryGroupRow {
        kind_label: kind.into(),
        participants: vec![ParticipantRow {
            function_name: fname.into(),
            file: file.into(),
            line,
        }],
    }
}

#[test]
fn dry_group_categories_render_with_participants() {
    let mut v = empty_view();
    v.duplicate_groups = vec![group("Exact", "dupfn", "d.rs", 1)];
    let out = format_dry_section(&v);
    assert!(out.contains("Exact duplicate"), "exact label (`==`): {out}");
    assert!(out.contains("dupfn (d.rs:1)"), "participant row: {out}");

    let mut v = empty_view();
    v.fragment_groups = vec![group("3", "fragfn", "f.rs", 2)];
    let out = format_dry_section(&v);
    assert!(out.contains("Fragment 1: 3 matching"), "{out}");
    assert!(out.contains("fragfn (f.rs:2)"), "{out}");

    let mut v = empty_view();
    v.repeated_match_groups = vec![group("E", "repfn", "r.rs", 6)];
    let out = format_dry_section(&v);
    assert!(out.contains("Repeated match [E]"), "{out}");
    assert!(out.contains("repfn (r.rs:6)"), "{out}");
}

#[test]
fn dry_dead_code_boilerplate_wildcard_render() {
    let mut v = empty_view();
    v.dead_code = vec![DeadCodeRow {
        qualified_name: "deadfn".into(),
        kind_tag: "uncalled",
        file: "x.rs".into(),
        line: 3,
        suggestion: "remove it".into(),
    }];
    assert!(
        format_dry_section(&v).contains("deadfn [uncalled]"),
        "dead code row"
    );

    let mut v = empty_view();
    v.boilerplate = vec![BoilerplateRow {
        pattern_id: "BP-001".into(),
        struct_name: "Foo".into(),
        file: "b.rs".into(),
        line: 4,
        message: "boilerplate msg".into(),
        suggestion: "sug".into(),
    }];
    assert!(
        format_dry_section(&v).contains("[BP-001] Foo"),
        "boilerplate row"
    );

    let mut v = empty_view();
    v.wildcards = vec![WildcardRow {
        module_path: "foo::*".into(),
        file: "w.rs".into(),
        line: 5,
    }];
    assert!(
        format_dry_section(&v).contains("Wildcard import: foo::*"),
        "wildcard row"
    );
}

#[test]
fn dry_section_empty_when_no_findings() {
    assert!(format_dry_section(&empty_view()).is_empty());
}
