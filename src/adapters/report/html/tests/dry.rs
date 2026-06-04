//! HTML DRY section rendering: the finding-total `+` sum across the six
//! categories and each per-category table/group builder.
use crate::adapters::report::projections::dry::{
    BoilerplateRow, DeadCodeRow, DryGroupRow, ParticipantRow, WildcardRow,
};
use crate::report::html::dry::format_dry_section;
use crate::report::html::views::HtmlDryView;

fn group(kind: &str, fname: &str) -> DryGroupRow {
    DryGroupRow {
        kind_label: kind.into(),
        participants: vec![ParticipantRow {
            function_name: fname.into(),
            file: "lib.rs".into(),
            line: 1,
        }],
    }
}

#[test]
fn dry_section_total_and_every_category_table() {
    // One entry in each of the six categories → total 6 (pins the five `+`).
    let view = HtmlDryView {
        duplicate_groups: vec![group("Exact", "dupfn")],
        fragment_groups: vec![group("3", "fragfn")],
        repeated_match_groups: vec![group("E", "repfn")],
        dead_code: vec![DeadCodeRow {
            qualified_name: "deadfn".into(),
            kind_tag: "uncalled",
            file: "d.rs".into(),
            line: 2,
            suggestion: "remove".into(),
        }],
        boilerplate: vec![BoilerplateRow {
            pattern_id: "BP-001".into(),
            struct_name: "Foo".into(),
            file: "b.rs".into(),
            line: 3,
            message: "boiler".into(),
            suggestion: "derive".into(),
        }],
        wildcards: vec![WildcardRow {
            module_path: "m::*".into(),
            file: "w.rs".into(),
            line: 5,
        }],
    };
    let html = format_dry_section(&view);
    assert!(
        html.contains("DRY \u{2014} 6 Findings"),
        "total header: {html}"
    );
    assert!(html.contains("dupfn"), "duplicate group: {html}");
    assert!(html.contains("fragfn"), "fragment group: {html}");
    assert!(html.contains("repfn"), "repeated-match group: {html}");
    assert!(html.contains("deadfn"), "dead-code table: {html}");
    assert!(html.contains("BP-001"), "boilerplate table: {html}");
    assert!(html.contains("m::*"), "wildcard table: {html}");
}

#[test]
fn dry_section_empty_state() {
    let view = HtmlDryView {
        duplicate_groups: vec![],
        fragment_groups: vec![],
        repeated_match_groups: vec![],
        dead_code: vec![],
        boilerplate: vec![],
        wildcards: vec![],
    };
    let html = format_dry_section(&view);
    assert!(html.contains("DRY \u{2014} 0 Findings"), "{html}");
    assert!(html.contains("No DRY issues found"), "{html}");
}
