//! Pipeline-level glue for the port-based Architecture dimension.
//!
//! The primary `run_analysis` function delegates the Architecture pass to
//! this module to keep its surface area small. The work here is pure
//! adapter-wiring:
//!   1. Project the parsed workspace into `ParsedFile`s.
//!   2. Build an `AnalysisContext` and hand it to `app::analyze_codebase`
//!      with the Architecture adapter in the analyzer list.
//!   3. Apply file-level `qual:allow(architecture)` suppressions.
//!   4. Sort the findings stably and update the `Summary` counter.

use crate::adapters::analyzers::architecture::ArchitectureAnalyzer;
use crate::config::Config;
use crate::domain::Finding;
use crate::findings::{Dimension, Suppression};
use crate::ports::{AnalysisContext, DimensionAnalyzer, ParsedFile};
use crate::report::Summary;
use std::collections::HashMap;

/// Run the Architecture dimension, apply suppressions, and update the summary.
/// Integration: delegates to run + count + sort helpers.
pub(super) fn collect_architecture_findings(
    parsed: &[(String, String, syn::File)],
    config: &Config,
    suppression_lines: &HashMap<String, Vec<Suppression>>,
    summary: &mut Summary,
) -> Vec<Finding> {
    let mut findings = run_architecture_dimension(parsed, config, suppression_lines);
    summary.architecture_warnings = findings.iter().filter(|f| !f.suppressed).count();
    findings.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });
    findings
}

/// Run every port-based dimension analyzer and return their findings with
/// file-level suppressions applied.
/// Integration: delegates context construction, analyzer dispatch, suppression marking.
fn run_architecture_dimension(
    parsed: &[(String, String, syn::File)],
    config: &Config,
    suppression_lines: &HashMap<String, Vec<Suppression>>,
) -> Vec<Finding> {
    let files: Vec<ParsedFile> = parsed
        .iter()
        .map(|(p, c, f)| ParsedFile {
            path: p.clone(),
            content: c.clone(),
            ast: f.clone(),
        })
        .collect();
    let ctx = AnalysisContext {
        files: &files,
        config,
    };
    let analyzers: Vec<Box<dyn DimensionAnalyzer>> = vec![Box::new(ArchitectureAnalyzer)];
    let mut findings = crate::app::analyze_codebase(&analyzers, &ctx);
    mark_architecture_suppressions(&mut findings, suppression_lines);
    findings
}

/// Mark findings whose annotation window contains a
/// `// qual:allow(architecture)` suppression.
///
/// Window-scoped (not file-scoped): a suppression at line N covers
/// findings at lines `N..=N+ANNOTATION_WINDOW`. Otherwise a single
/// `qual:allow(architecture)` for one helper would silence unrelated
/// call-parity, layer, or forbidden-edge findings elsewhere in the
/// same file. Operation: per-finding lookup over the suppression map.
fn mark_architecture_suppressions(
    findings: &mut [Finding],
    suppression_lines: &HashMap<String, Vec<Suppression>>,
) {
    findings.iter_mut().for_each(|f| {
        let suppressed = suppression_lines
            .get(&f.file)
            .map(|sups| {
                sups.iter().any(|s| {
                    s.covers(Dimension::Architecture)
                        && crate::findings::is_within_window(s.line, f.line)
                })
            })
            .unwrap_or(false);
        if suppressed {
            f.suppressed = true;
        }
    });
}
