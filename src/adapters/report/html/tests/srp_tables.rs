//! HTML SRP sub-table rendering tests: the warning-total header arithmetic,
//! each sub-table builder, and the `independent_clusters > 0` cluster cell.
use crate::adapters::report::projections::srp::{SrpModuleRow, SrpParamRow, SrpStructRow};
use crate::report::html::srp_tables::format_srp_section;
use crate::report::html::views::HtmlSrpView;

fn struct_row() -> SrpStructRow {
    SrpStructRow {
        struct_name: "BigStruct".into(),
        file: "s.rs".into(),
        line: 3,
        lcom4: 4,
        field_count: 9,
        method_count: 7,
        fan_out: 2,
    }
}

fn module_row(independent_clusters: usize) -> SrpModuleRow {
    SrpModuleRow {
        module: "big_mod".into(),
        file: "m.rs".into(),
        production_lines: 900,
        independent_clusters,
        cluster_names: vec![],
    }
}

fn param_row() -> SrpParamRow {
    SrpParamRow {
        function_name: "wide_fn".into(),
        file: "p.rs".into(),
        line: 8,
        parameter_count: 7,
    }
}

#[test]
fn srp_section_total_header_and_subtables() {
    // 1 struct + 2 module + 1 param = 4 warnings; the header sum pins the two `+`.
    let view = HtmlSrpView {
        struct_warnings: vec![struct_row()],
        module_warnings: vec![module_row(2), module_row(0)],
        param_warnings: vec![param_row()],
        structural_rows: vec![],
    };
    let html = format_srp_section(&view);
    assert!(
        html.contains("SRP \u{2014} 4 Warnings"),
        "total header: {html}"
    );
    // Each sub-table renders its row content (pins the `-> String` builders).
    assert!(html.contains("BigStruct"), "struct row: {html}");
    assert!(html.contains("big_mod"), "module row: {html}");
    assert!(html.contains("wide_fn"), "param row: {html}");
    // Cluster cell: clusters>0 → "N clusters", clusters==0 → em-dash. Pins `> 0`
    // against `>=` (a zero-cluster module must NOT render "0 clusters").
    assert!(html.contains("2 clusters"), "non-zero clusters: {html}");
    assert!(
        !html.contains("0 clusters"),
        "zero clusters → em-dash, not '0 clusters': {html}"
    );
}

#[test]
fn srp_section_empty_states_when_no_warnings() {
    let view = HtmlSrpView {
        struct_warnings: vec![],
        module_warnings: vec![],
        param_warnings: vec![],
        structural_rows: vec![],
    };
    let html = format_srp_section(&view);
    assert!(html.contains("SRP \u{2014} 0 Warnings"), "{html}");
    assert!(html.contains("No SRP warnings"), "empty state: {html}");
}
