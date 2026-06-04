//! HTML rendering tests for the Coupling, Structural, and Test-Quality
//! sections — each `format_*_subsection`/`format_*` builder must reach the
//! output, and the structural warning-total `+` and `&&` guards are pinned.
use crate::adapters::report::projections::coupling::SdpViolationRow;
use crate::adapters::report::projections::srp::StructuralRow;
use crate::adapters::report::projections::tq::TqRow;
use crate::report::html::coupling::format_coupling_section;
use crate::report::html::structural_table::format_structural_section;
use crate::report::html::tq_table::format_tq_section;
use crate::report::html::views::{
    HtmlCouplingDataView, HtmlCouplingModuleRow, HtmlCouplingView, HtmlTqView,
};

fn srow(code: &str, name: &str) -> StructuralRow {
    StructuralRow {
        code: code.into(),
        name: name.into(),
        detail: "detail".into(),
        file: "s.rs".into(),
        line: 4,
    }
}

#[test]
fn coupling_section_renders_cycles_sdp_and_modules() {
    let view = HtmlCouplingView {
        cycle_paths: vec![vec!["cyc_alpha".into(), "cyc_beta".into()]],
        sdp_violations: vec![SdpViolationRow {
            from: "sdp_from".into(),
            from_instability: 0.2,
            to: "sdp_to".into(),
            to_instability: 0.8,
        }],
        structural_rows: vec![],
    };
    let data = HtmlCouplingDataView {
        modules: vec![HtmlCouplingModuleRow {
            name: "mod_node".into(),
            afferent: 1,
            efferent: 9,
            instability: 0.9,
            suppressed: false,
        }],
    };
    let html = format_coupling_section(&view, &data);
    assert!(html.contains("1 Module, 1 Cycle"), "header: {html}");
    assert!(html.contains("mod_node"), "module subsection: {html}");
    assert!(
        html.contains("sdp_from") && html.contains("sdp_to"),
        "sdp subsection: {html}"
    );
    assert!(
        html.contains("cyc_alpha") && html.contains("cyc_beta"),
        "cycle subsection: {html}"
    );
}

#[test]
fn structural_section_total_and_rows() {
    // 1 SRP + 1 Coupling row = 2 warnings; the header sum pins the `+`.
    let html = format_structural_section(&[srow("SLM", "Foo")], &[srow("OI", "Bar")]);
    assert!(html.contains("2 Warning"), "total header: {html}");
    assert!(
        html.contains("SLM") && html.contains("Foo"),
        "srp row: {html}"
    );
    assert!(
        html.contains("OI") && html.contains("Bar"),
        "coupling row: {html}"
    );
    // An SRP-only set still renders its row (pins the all-empty `&&` guard) and
    // pluralizes "Warning" via `count == 1` (one row → singular).
    let srp_only = format_structural_section(&[srow("BTC", "Trait")], &[]);
    assert!(srp_only.contains("BTC"), "srp-only renders: {srp_only}");
    assert!(
        srp_only.contains("1 Warning<"),
        "single row → singular 'Warning' (pins `count == 1`): {srp_only}"
    );
    // Both empty → no warning rows.
    let empty = format_structural_section(&[], &[]);
    assert!(empty.contains("No structural warnings"), "empty: {empty}");
}

#[test]
fn tq_section_renders_row() {
    let view = HtmlTqView {
        warnings: vec![TqRow {
            function_name: "test_it".into(),
            file: "t.rs".into(),
            line: 9,
            display_label: "no assertion",
            detail: String::new(),
        }],
    };
    let html = format_tq_section(&view);
    assert!(html.contains("test_it"), "tq row: {html}");
    assert!(html.contains("no assertion"), "tq label: {html}");
}
