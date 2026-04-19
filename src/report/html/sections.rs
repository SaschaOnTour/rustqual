use super::html_escape;
use crate::adapters::analyzers::iosp::{
    Classification, FunctionAnalysis, Severity, PERCENTAGE_MULTIPLIER,
};
use crate::report::Summary;

/// Build an HTML table row for a single IOSP violation.
/// Operation: formatting logic, no own calls (html_escape via closure parameter).
fn html_violation_row(f: &FunctionAnalysis, esc: &dyn Fn(&str) -> String) -> String {
    let (logic, calls) = match &f.classification {
        Classification::Violation {
            logic_locations,
            call_locations,
            ..
        } => {
            let l: Vec<String> = logic_locations.iter().map(|l| l.to_string()).collect();
            let c: Vec<String> = call_locations.iter().map(|c| c.to_string()).collect();
            (l.join(", "), c.join(", "))
        }
        _ => (String::new(), String::new()),
    };
    let (sc, st) = match &f.severity {
        Some(Severity::High) => ("severity-high", "High"),
        Some(Severity::Medium) => ("severity-medium", "Medium"),
        Some(Severity::Low) => ("severity-low", "Low"),
        None => ("", "\u{2014}"),
    };
    let effort = f
        .effort_score
        .map(|e| format!("{e:.1}"))
        .unwrap_or_default();
    format!(
        "<tr><td>{}</td><td>{}</td><td>{}</td>\
         <td class=\"{sc}\">{st}</td><td>{effort}</td><td>{}</td><td>{}</td></tr>\n",
        esc(&f.qualified_name),
        esc(&f.file),
        f.line,
        esc(&logic),
        esc(&calls),
    )
}

/// Build the IOSP violations section.
/// Operation: iteration, filtering, and HTML formatting logic.
pub(super) fn html_iosp_section(results: &[FunctionAnalysis], summary: &Summary) -> String {
    let esc = |s: &str| html_escape(s);
    let row = |f: &FunctionAnalysis| html_violation_row(f, &esc);
    let vc = summary.violations;
    let mut html = String::new();
    html.push_str(&format!(
        "<details{}>\n<summary>IOSP \u{2014} {} Violation{}, {:.1}% Score</summary>\n\
         <div class=\"detail-content\">\n",
        if vc > 0 { " open" } else { "" },
        vc,
        if vc == 1 { "" } else { "s" },
        summary.iosp_score * PERCENTAGE_MULTIPLIER,
    ));
    if vc == 0 {
        html.push_str("<p class=\"empty-state\">No IOSP violations.</p>\n");
    } else {
        html.push_str(
            "<table>\n<thead><tr><th>Function</th><th>File</th><th>Line</th>\
             <th>Severity</th><th>Effort</th><th>Logic</th><th>Calls</th></tr></thead>\n<tbody>\n",
        );
        results
            .iter()
            .filter(|f| {
                !f.suppressed && matches!(f.classification, Classification::Violation { .. })
            })
            .for_each(|f| html.push_str(&row(f)));
        html.push_str("</tbody></table>\n");
    }
    html.push_str("</div>\n</details>\n\n");
    html
}

/// Build an HTML table row for a single complexity warning.
/// Operation: formatting logic, no own calls (html_escape via closure parameter).
fn html_complexity_row(
    f: &FunctionAnalysis,
    m: &crate::adapters::analyzers::iosp::ComplexityMetrics,
    esc: &dyn Fn(&str) -> String,
) -> String {
    let magic_issue = (!m.magic_numbers.is_empty()).then(|| {
        let mn: Vec<String> = m.magic_numbers.iter().map(|n| n.to_string()).collect();
        format!("magic: {}", esc(&mn.join(", ")))
    });
    let unsafe_issue = (m.unsafe_blocks > 0).then(|| format!("{} unsafe", m.unsafe_blocks));
    let err_parts: Vec<String> = [
        (m.unwrap_count, "unwrap"),
        (m.expect_count, "expect"),
        (m.panic_count, "panic"),
        (m.todo_count, "todo"),
    ]
    .iter()
    .filter(|(c, _)| *c > 0)
    .map(|(c, l)| format!("{c}{l}"))
    .collect();
    let err_issue = (!err_parts.is_empty()).then(|| err_parts.join(", "));
    let issues: Vec<&str> = [&magic_issue, &unsafe_issue, &err_issue]
        .iter()
        .filter_map(|o| o.as_ref().map(|s| s.as_str()))
        .collect();
    let issue_str = if issues.is_empty() {
        "\u{2014}".to_string()
    } else {
        issues.join("; ")
    };
    format!(
        "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>\
         <td>{}</td><td>{}</td><td>{}</td></tr>\n",
        esc(&f.qualified_name),
        esc(&f.file),
        f.line,
        m.cognitive_complexity,
        m.cyclomatic_complexity,
        m.max_nesting,
        m.function_lines,
        issue_str,
    )
}

/// Build the complexity warnings section.
/// Operation: iteration, filtering, and HTML formatting logic.
pub(super) fn html_complexity_section(results: &[FunctionAnalysis]) -> String {
    let esc = |s: &str| html_escape(s);
    let row = |f: &FunctionAnalysis, m: &crate::adapters::analyzers::iosp::ComplexityMetrics| {
        html_complexity_row(f, m, &esc)
    };
    let has_warn = |f: &FunctionAnalysis| {
        [
            f.cognitive_warning,
            f.cyclomatic_warning,
            f.nesting_depth_warning,
            f.function_length_warning,
            f.unsafe_warning,
            f.error_handling_warning,
            f.complexity
                .as_ref()
                .is_some_and(|c| !c.magic_numbers.is_empty()),
        ]
        .iter()
        .any(|&b| b)
    };
    let warnings: Vec<&FunctionAnalysis> = results
        .iter()
        .filter(|f| !f.suppressed && !f.complexity_suppressed && has_warn(f))
        .collect();
    let mut html = String::new();
    html.push_str(&format!(
        "<details>\n<summary>Complexity \u{2014} {} Warning{}</summary>\n\
         <div class=\"detail-content\">\n",
        warnings.len(),
        if warnings.len() == 1 { "" } else { "s" },
    ));
    if warnings.is_empty() {
        html.push_str("<p class=\"empty-state\">No complexity warnings.</p>\n");
    } else {
        html.push_str(
            "<table>\n<thead><tr><th>Function</th><th>File</th><th>Line</th>\
             <th>Cognitive</th><th>Cyclomatic</th><th>Nesting</th>\
             <th>Lines</th><th>Issues</th></tr></thead>\n<tbody>\n",
        );
        warnings
            .iter()
            .filter_map(|f| f.complexity.as_ref().map(|m| (*f, m)))
            .for_each(|(f, m)| html.push_str(&row(f, m)));
        html.push_str("</tbody></table>\n");
    }
    html.push_str("</div>\n</details>\n\n");
    html
}

/// Format a single module metrics row.
/// Operation: conditional formatting logic, no own calls.
fn html_module_metric_row(
    m: &crate::adapters::analyzers::coupling::CouplingMetrics,
    esc: &dyn Fn(&str) -> String,
) -> String {
    let st = if m.suppressed {
        "<span class=\"tag tag-ok\">suppressed</span>"
    } else {
        ""
    };
    format!(
        "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.2}</td><td>{}</td></tr>\n",
        esc(&m.module_name),
        m.afferent,
        m.efferent,
        m.instability,
        st,
    )
}

/// Build coupling sub-sections: cycles, SDP violations, and metrics table.
/// Operation: conditional iteration and HTML formatting logic; helper call in closure.
fn html_coupling_subsections(
    coupling: Option<&crate::adapters::analyzers::coupling::CouplingAnalysis>,
    cc: usize,
    esc: &dyn Fn(&str) -> String,
) -> String {
    let metric_row =
        |m: &crate::adapters::analyzers::coupling::CouplingMetrics| html_module_metric_row(m, esc);
    let mut html = String::new();
    if cc > 0 {
        html.push_str("<h3>Circular Dependencies</h3>\n<ul>\n");
        coupling
            .iter()
            .flat_map(|c| c.cycles.iter())
            .for_each(|cycle| {
                let path: Vec<String> = cycle.modules.iter().map(|m| esc(m)).collect();
                html.push_str(&format!(
                    "  <li class=\"severity-high\">{}</li>\n",
                    path.join(" \u{2192} ")
                ));
            });
        html.push_str("</ul>\n");
    }
    let sdp: Vec<_> = coupling
        .iter()
        .flat_map(|c| c.sdp_violations.iter().filter(|v| !v.suppressed))
        .collect();
    if !sdp.is_empty() {
        html.push_str(
            "<h3>SDP Violations</h3>\n<table>\n<thead><tr>\
             <th>From (stable)</th><th>Instability</th>\
             <th>To (unstable)</th><th>Instability</th>\
             </tr></thead>\n<tbody>\n",
        );
        sdp.iter().for_each(|v| {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{:.2}</td><td>{}</td><td>{:.2}</td></tr>\n",
                esc(&v.from_module),
                v.from_instability,
                esc(&v.to_module),
                v.to_instability,
            ));
        });
        html.push_str("</tbody></table>\n");
    }
    html.push_str(
        "<h3>Module Metrics</h3>\n<table>\n<thead><tr><th>Module</th>\
         <th>Fan-in</th><th>Fan-out</th><th>Instability</th><th>Status</th>\
         </tr></thead>\n<tbody>\n",
    );
    coupling
        .iter()
        .flat_map(|c| c.metrics.iter())
        .for_each(|m| html.push_str(&metric_row(m)));
    html.push_str("</tbody></table>\n");
    html
}

/// Build the coupling analysis section.
/// Operation: conditional formatting logic, calls helper via closure.
pub(super) fn html_coupling_section(
    coupling: Option<&crate::adapters::analyzers::coupling::CouplingAnalysis>,
) -> String {
    let esc = |s: &str| html_escape(s);
    let subsections = |c, cc| html_coupling_subsections(c, cc, &esc);
    let (mc, cc) = coupling
        .map(|c| (c.metrics.len(), c.cycles.len()))
        .unwrap_or((0, 0));
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
        html.push_str(&subsections(coupling, cc));
    }
    html.push_str("</div>\n</details>\n\n");
    html
}
