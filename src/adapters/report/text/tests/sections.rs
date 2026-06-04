//! Rendering tests for the smaller text sections: cross-dimension Structural,
//! Architecture (view projection + section), Test Quality rows, and the
//! findings-list heading.
use crate::adapters::report::projections::srp::StructuralRow;
use crate::adapters::report::projections::tq::TqRow;
use crate::domain::findings::ArchitectureFinding;
use crate::domain::{Dimension, Finding, Severity};
use crate::report::findings_list::FindingEntry;
use crate::report::text::architecture::{build_architecture_view, format_architecture_section};
use crate::report::text::format_findings_list;
use crate::report::text::structural::format_structural_section;
use crate::report::text::tq::format_tq_section;
use crate::report::text::views::{ArchitectureView, TqView};

fn srow(code: &str, name: &str) -> StructuralRow {
    StructuralRow {
        code: code.into(),
        name: name.into(),
        detail: "detail text".into(),
        file: "s.rs".into(),
        line: 7,
    }
}

#[test]
fn structural_section_renders_either_side_and_is_empty_when_both_empty() {
    // An SRP-only set renders (pins `srp.is_empty() && coupling.is_empty()`
    // against `||`, which would early-return on the empty coupling side), and
    // the row content pins the `-> String::new()`/"xyzzy" body replacements.
    let out = format_structural_section(&[srow("SLM", "Foo::bar")], &[]);
    assert!(out.contains("SLM"), "{out}");
    assert!(out.contains("Foo::bar (s.rs:7)"), "{out}");
    assert!(out.contains("detail text"), "{out}");
    // Coupling-only side also renders.
    let out = format_structural_section(&[], &[srow("OI", "Baz")]);
    assert!(out.contains("OI") && out.contains("Baz"), "{out}");
    // Both empty → nothing.
    assert!(format_structural_section(&[], &[]).is_empty());
}

fn arch_finding(file: &str, suppressed: bool) -> ArchitectureFinding {
    ArchitectureFinding {
        common: Finding {
            file: file.into(),
            line: 5,
            column: 0,
            dimension: Dimension::Architecture,
            rule_id: "architecture/layer".into(),
            message: "layer violation".into(),
            severity: Severity::Medium,
            suppressed,
        },
    }
}

#[test]
fn architecture_view_drops_suppressed_and_section_renders_rows() {
    // build_architecture_view keeps only unsuppressed findings (pins the `!`),
    // and the section renders each with a singular/plural heading.
    let view = build_architecture_view(&[
        arch_finding("keep.rs", false),
        arch_finding("drop.rs", true),
    ]);
    assert_eq!(view.findings.len(), 1, "suppressed finding dropped");
    let out = format_architecture_section(&view);
    assert!(out.contains("1 Finding"), "singular heading: {out}");
    assert!(out.contains("keep.rs"), "{out}");
    assert!(out.contains("architecture/layer"), "{out}");
    assert!(!out.contains("drop.rs"), "suppressed row absent: {out}");
    // Empty view → empty output.
    assert!(format_architecture_section(&ArchitectureView { findings: vec![] }).is_empty());
}

#[test]
fn tq_section_renders_row_with_detail() {
    // A TQ row with a non-empty detail renders both the main line and the detail
    // sub-line. Pins `push_tq_row` (no-op) and the `!detail.is_empty()` guard.
    let view = TqView {
        warnings: vec![TqRow {
            function_name: "test_it".into(),
            file: "t.rs".into(),
            line: 9,
            display_label: "no assertion",
            detail: "missing assert".into(),
        }],
    };
    let out = format_tq_section(&view);
    assert!(out.contains("test_it (t.rs:9)"), "{out}");
    assert!(out.contains("no assertion"), "{out}");
    assert!(out.contains("missing assert"), "detail sub-line: {out}");
}

#[test]
fn findings_list_heading_pluralizes_and_lists_entries() {
    // Two entries → plural "2 Findings" heading and each entry line. Pins
    // format_findings_list against the `-> String::new()`/"xyzzy" replacements.
    let entries = vec![
        FindingEntry {
            file: "a.rs".into(),
            line: 1,
            category: "COGNITIVE",
            detail: String::new(),
            function_name: "fa".into(),
        },
        FindingEntry {
            file: "b.rs".into(),
            line: 2,
            category: "DUPLICATE",
            detail: String::new(),
            function_name: "fb".into(),
        },
    ];
    let out = format_findings_list(&entries);
    assert!(out.contains("2 Findings"), "plural heading: {out}");
    assert!(out.contains("a.rs:1") && out.contains("COGNITIVE"), "{out}");
    assert!(out.contains("b.rs:2") && out.contains("DUPLICATE"), "{out}");
    assert!(format_findings_list(&[]).is_empty());
}
