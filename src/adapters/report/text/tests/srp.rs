//! Rendering tests for the text SRP section. Single-populated-category views
//! pin each `push_*` (no-op) and the all-empty `&&` early-return guard; the
//! module rows additionally pin the `production_lines > 0` /
//! `independent_clusters > 0` sub-guards.
use crate::adapters::report::projections::srp::{SrpModuleRow, SrpParamRow, SrpStructRow};
use crate::report::text::srp::format_srp_section;
use crate::report::text::views::SrpView;

fn empty_view() -> SrpView {
    SrpView {
        struct_warnings: vec![],
        module_warnings: vec![],
        param_warnings: vec![],
        structural_rows: vec![],
    }
}

#[test]
fn srp_struct_and_param_rows_render() {
    let mut v = empty_view();
    v.struct_warnings = vec![SrpStructRow {
        struct_name: "Big".into(),
        file: "s.rs".into(),
        line: 3,
        lcom4: 4,
        field_count: 9,
        method_count: 7,
        fan_out: 2,
    }];
    let out = format_srp_section(&v);
    assert!(out.contains("Big (s.rs:3)"), "{out}");
    assert!(out.contains("LCOM4=4"), "{out}");

    let mut v = empty_view();
    v.param_warnings = vec![SrpParamRow {
        function_name: "big_fn".into(),
        file: "p.rs".into(),
        line: 8,
        parameter_count: 7,
    }];
    let out = format_srp_section(&v);
    assert!(out.contains("big_fn (p.rs:8)"), "{out}");
    assert!(out.contains("7 parameters"), "{out}");
}

#[test]
fn srp_module_rows_show_only_nonzero_lines_and_clusters() {
    // One module with positive lines + clusters renders both detail lines and the
    // cluster names; a second module with zero of each renders nothing. Pins the
    // `production_lines > 0` and `independent_clusters > 0` guards.
    let mut v = empty_view();
    v.module_warnings = vec![
        SrpModuleRow {
            module: "big".into(),
            file: "m.rs".into(),
            production_lines: 900,
            independent_clusters: 2,
            cluster_names: vec!["a, b".into()],
        },
        SrpModuleRow {
            module: "small".into(),
            file: "m2.rs".into(),
            production_lines: 0,
            independent_clusters: 0,
            cluster_names: vec![],
        },
    ];
    let out = format_srp_section(&v);
    assert!(out.contains("big — 900 production lines"), "{out}");
    assert!(
        out.contains("big — 2 independent function clusters"),
        "{out}"
    );
    assert!(out.contains("Cluster 1: [a, b]"), "{out}");
    assert!(
        !out.contains("small —"),
        "zero module renders no detail: {out}"
    );
}

#[test]
fn srp_section_empty_when_no_findings() {
    assert!(format_srp_section(&empty_view()).is_empty());
}
