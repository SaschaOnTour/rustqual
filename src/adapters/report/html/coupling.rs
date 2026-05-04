//! HTML coupling section: per-module metrics table, cycles, SDP
//! violations.

use super::html_escape;
use super::views::{HtmlCouplingDataView, HtmlCouplingModuleRow, HtmlCouplingView};
use crate::adapters::report::projections::coupling::{split_coupling_findings, SdpViolationRow};
use crate::domain::analysis_data::ModuleCouplingRecord;
use crate::domain::findings::CouplingFinding;

/// Project Coupling findings into the typed view via the shared
/// `split_coupling_findings` helper.
pub(super) fn build_coupling_view(findings: &[CouplingFinding]) -> HtmlCouplingView {
    let buckets = split_coupling_findings(findings);
    HtmlCouplingView {
        cycle_paths: buckets.cycle_paths,
        sdp_violations: buckets.sdp_violations,
        structural_rows: buckets.structural_rows,
    }
}

/// Project module records into the typed view.
pub(super) fn build_coupling_data_view(modules: &[ModuleCouplingRecord]) -> HtmlCouplingDataView {
    let modules = modules
        .iter()
        .map(|m| HtmlCouplingModuleRow {
            name: m.module_name.clone(),
            afferent: m.afferent,
            efferent: m.efferent,
            instability: m.instability,
            suppressed: m.suppressed,
        })
        .collect();
    HtmlCouplingDataView { modules }
}

pub(super) fn format_coupling_section(
    findings: &HtmlCouplingView,
    table: &HtmlCouplingDataView,
) -> String {
    let esc = |s: &str| html_escape(s);
    let mc = table.modules.len();
    let cc = findings.cycle_paths.len();
    let mut html = String::new();
    html.push_str(&format!(
        "<details>\n<summary>Coupling \u{2014} {} Module{}, {} Cycle{}</summary>\n\
         <div class=\"detail-content\">\n",
        mc,
        if mc == 1 { "" } else { "s" },
        cc,
        if cc == 1 { "" } else { "s" },
    ));
    if mc == 0 {
        html.push_str("<p class=\"empty-state\">No coupling data.</p>\n");
    } else {
        html.push_str(&format_subsections(findings, table, &esc));
    }
    html.push_str("</div>\n</details>\n\n");
    html
}

fn format_subsections(
    findings: &HtmlCouplingView,
    table: &HtmlCouplingDataView,
    esc: &dyn Fn(&str) -> String,
) -> String {
    let mut html = String::new();
    html.push_str(&format_cycles_subsection(&findings.cycle_paths, esc));
    html.push_str(&format_sdp_subsection(&findings.sdp_violations, esc));
    html.push_str(&format_modules_subsection(&table.modules, esc));
    html
}

fn format_cycles_subsection(cycles: &[Vec<String>], esc: &dyn Fn(&str) -> String) -> String {
    if cycles.is_empty() {
        return String::new();
    }
    let mut html = String::from("<h3>Circular Dependencies</h3>\n<ul>\n");
    cycles.iter().for_each(|cycle| {
        let path: Vec<String> = cycle.iter().map(|m| esc(m)).collect();
        html.push_str(&format!(
            "  <li class=\"severity-high\">{}</li>\n",
            path.join(" \u{2192} ")
        ));
    });
    html.push_str("</ul>\n");
    html
}

fn format_sdp_subsection(rows: &[SdpViolationRow], esc: &dyn Fn(&str) -> String) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut html = String::new();
    html.push_str(
        "<h3>SDP Violations</h3>\n<table>\n<thead><tr>\
         <th>From (stable)</th><th>Instability</th>\
         <th>To (unstable)</th><th>Instability</th>\
         </tr></thead>\n<tbody>\n",
    );
    rows.iter().for_each(|r| {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{:.2}</td><td>{}</td><td>{:.2}</td></tr>\n",
            esc(&r.from),
            r.from_instability,
            esc(&r.to),
            r.to_instability,
        ));
    });
    html.push_str("</tbody></table>\n");
    html
}

fn format_modules_subsection(
    modules: &[HtmlCouplingModuleRow],
    esc: &dyn Fn(&str) -> String,
) -> String {
    let mut html = String::new();
    html.push_str(
        "<h3>Module Metrics</h3>\n<table>\n<thead><tr><th>Module</th>\
         <th>Fan-in</th><th>Fan-out</th><th>Instability</th><th>Status</th>\
         </tr></thead>\n<tbody>\n",
    );
    modules.iter().for_each(|m| {
        let st = if m.suppressed {
            "<span class=\"tag tag-ok\">suppressed</span>"
        } else {
            ""
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.2}</td><td>{}</td></tr>\n",
            esc(&m.name),
            m.afferent,
            m.efferent,
            m.instability,
            st,
        ));
    });
    html.push_str("</tbody></table>\n");
    html
}
