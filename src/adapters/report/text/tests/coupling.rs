//! Rendering tests for the text Coupling section. A verbose render with a
//! warning module, a cycle, an SDP violation, and incoming/outgoing edges pins
//! every `push_*` sub-section; a non-verbose render pins the `verbose && !…`
//! detail guards and the cycle-status line.
use crate::adapters::report::projections::coupling::SdpViolationRow;
use crate::report::text::coupling::format_coupling_section;
use crate::report::text::views::{CouplingTableView, CouplingView, ModuleRow};

fn module(name: &str, warning: bool) -> ModuleRow {
    ModuleRow {
        name: name.into(),
        afferent: 1,
        efferent: 9,
        instability: 0.9,
        suppressed: false,
        warning,
        incoming: vec!["up".into()],
        outgoing: vec!["down".into()],
    }
}

fn view_with_cycle() -> CouplingView {
    CouplingView {
        cycle_paths: vec![vec!["a".into(), "b".into()]],
        sdp_violations: vec![SdpViolationRow {
            from: "x".into(),
            from_instability: 0.2,
            to: "y".into(),
            to_instability: 0.8,
        }],
        structural_rows: vec![],
    }
}

#[test]
fn coupling_verbose_renders_header_cycle_sdp_legend_and_module_edges() {
    let view = view_with_cycle();
    let table = CouplingTableView {
        modules: vec![module("modA", true)],
    };
    let out = format_coupling_section(&view, &table, true);
    assert!(out.contains("Modules analyzed: 1"), "verbose header: {out}");
    assert!(out.contains("Circular dependency: a → b"), "cycle: {out}");
    assert!(out.contains("SDP violation: x"), "sdp: {out}");
    assert!(out.contains("Incoming"), "legend: {out}");
    assert!(out.contains("modA"), "module row: {out}");
    assert!(out.contains("exceeds threshold"), "warning tag: {out}");
    assert!(
        out.contains("→ depends on:") && out.contains("down"),
        "outgoing: {out}"
    );
    assert!(
        out.contains("← used by:") && out.contains("up"),
        "incoming: {out}"
    );
    assert!(
        !out.contains("No circular dependencies"),
        "cycle present → no all-clear line: {out}"
    );
}

#[test]
fn coupling_non_verbose_hides_edges_and_shows_all_clear() {
    // No cycles, non-verbose: the compact table header shows, per-module
    // incoming/outgoing detail is hidden (pins `verbose && !…`), and the
    // all-clear cycle status line is emitted.
    let view = CouplingView {
        cycle_paths: vec![],
        sdp_violations: vec![],
        structural_rows: vec![],
    };
    let table = CouplingTableView {
        modules: vec![module("modB", false)],
    };
    let out = format_coupling_section(&view, &table, false);
    assert!(out.contains("Instability"), "compact table header: {out}");
    assert!(out.contains("modB"), "module row: {out}");
    assert!(
        !out.contains("depends on"),
        "edges hidden non-verbose: {out}"
    );
    assert!(!out.contains("used by"), "edges hidden non-verbose: {out}");
    assert!(
        out.contains("No circular dependencies"),
        "no cycles → all-clear line: {out}"
    );
}
