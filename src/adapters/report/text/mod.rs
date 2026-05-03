mod architecture;
mod coupling;
mod dry;
mod files;
mod srp;
pub(crate) mod structural;
mod summary;
pub(crate) mod tq;
mod views;

use std::fmt::Write;

use architecture::{build_architecture_view, format_architecture_section};
use coupling::{build_coupling_table_view, build_coupling_view, format_coupling_section};
use dry::{build_dry_view, format_dry_section};
use files::format_files_section;
use srp::{build_srp_view, format_srp_section};
use structural::format_structural_section;
use summary::format_summary_section;
use tq::{build_tq_view, format_tq_section};

use colored::Colorize;

use crate::adapters::analyzers::iosp::FunctionAnalysis;
use crate::domain::analysis_data::{FunctionRecord, ModuleCouplingRecord};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding, SrpFinding,
    TqFinding,
};
use crate::ports::reporter::{ReporterImpl, Snapshot};
use crate::report::findings_list::FindingEntry;

use super::Summary;

pub use files::format_files_section as files_section;
use views::{ArchitectureView, CouplingTableView, CouplingView, DryView, SrpView, TqView};

/// Text reporter — produces plain-text output for the terminal. Compact
/// mode shows summary + coupling table + flat findings list; verbose
/// mode adds per-file function listings + per-dim detail sections +
/// cross-dim Structural section.
///
/// `build_*` methods produce typed Views (pure data, no markup).
/// `publish` consumes the Views via `format_*_section` helpers and
/// composes the final string.
pub struct TextReporter<'a> {
    pub(crate) summary: &'a Summary,
    pub(crate) function_analyses: &'a [FunctionAnalysis],
    pub(crate) findings_entries: &'a [FindingEntry],
    pub(crate) verbose: bool,
    pub(crate) suggestions_text: Option<&'a str>,
}

impl<'a> ReporterImpl for TextReporter<'a> {
    type Output = String;

    type IospView = ();
    type ComplexityView = ();
    type DryView = DryView;
    type SrpView = SrpView;
    type CouplingView = CouplingView;
    type TestQualityView = TqView;
    type ArchitectureView = ArchitectureView;
    type IospDataView = ();
    type ComplexityDataView = ();
    type CouplingDataView = CouplingTableView;

    fn build_iosp(&self, _: &[IospFinding]) {}
    fn build_complexity(&self, _: &[ComplexityFinding]) {}
    fn build_dry(&self, findings: &[DryFinding]) -> DryView {
        build_dry_view(findings)
    }
    fn build_srp(&self, findings: &[SrpFinding]) -> SrpView {
        build_srp_view(findings)
    }
    fn build_coupling(&self, findings: &[CouplingFinding]) -> CouplingView {
        build_coupling_view(findings)
    }
    fn build_test_quality(&self, findings: &[TqFinding]) -> TqView {
        build_tq_view(findings)
    }
    fn build_architecture(&self, findings: &[ArchitectureFinding]) -> ArchitectureView {
        build_architecture_view(findings)
    }
    fn build_iosp_data(&self, _: &[FunctionRecord]) {}
    fn build_complexity_data(&self, _: &[FunctionRecord]) {}
    fn build_coupling_data(&self, modules: &[ModuleCouplingRecord]) -> CouplingTableView {
        build_coupling_table_view(modules)
    }

    fn publish(&self, snapshot: Snapshot<Self>) -> String {
        let Snapshot {
            iosp: (),
            complexity: (),
            dry,
            srp,
            coupling,
            test_quality,
            architecture,
            iosp_data: (),
            complexity_data: (),
            coupling_data,
        } = snapshot;
        let mut out = String::new();
        out.push_str(&format_summary_section(self.summary, self.findings_entries));
        out.push_str(&format_coupling_section(
            &coupling,
            &coupling_data,
            self.verbose,
        ));
        if self.verbose {
            out.push_str(&format_files_section(self.function_analyses, true));
            out.push_str(&format_dry_section(&dry));
            out.push_str(&format_srp_section(&srp));
            out.push_str(&format_tq_section(&test_quality));
            out.push_str(&format_structural_section(
                &srp.structural_rows,
                &coupling.structural_rows,
            ));
            out.push_str(&format_architecture_section(&architecture));
            out.push_str(&format_orphan_suppressions_section(self.findings_entries));
        } else {
            out.push_str(&format_findings_list(self.findings_entries));
        }
        if let Some(s) = self.suggestions_text {
            out.push_str(s);
        }
        out
    }
}

/// Format the findings list with heading.
fn format_findings_list(entries: &[FindingEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let n = entries.len();
    let heading = format!("\n═══ {} Finding{} ═══", n, if n == 1 { "" } else { "s" });
    let mut out = String::new();
    let _ = writeln!(out, "{}", heading.bold());
    entries.iter().for_each(|e| {
        let detail = if e.function_name.is_empty() {
            e.detail.clone()
        } else if e.detail.is_empty() {
            format!("in {}", e.function_name)
        } else {
            format!("{}  in {}", e.detail, e.function_name)
        };
        if e.file.is_empty() {
            let _ = writeln!(out, "  {}  {}", e.category, detail);
        } else {
            let _ = writeln!(out, "  {}:{}  {}  {}", e.file, e.line, e.category, detail);
        }
    });
    out
}

/// Verbose path renders one section per dimension; orphan suppressions
/// have no dedicated dimension section, so without this filtered list
/// a verbose run would fail (default-fail counts orphans) without
/// printing the file/line/reason needed to fix them.
fn format_orphan_suppressions_section(entries: &[FindingEntry]) -> String {
    let orphans: Vec<&FindingEntry> = entries
        .iter()
        .filter(|e| e.category == "ORPHAN_SUPPRESSION")
        .collect();
    if orphans.is_empty() {
        return String::new();
    }
    let n = orphans.len();
    let heading = format!(
        "\n═══ {} Orphan Suppression{} ═══",
        n,
        if n == 1 { "" } else { "s" }
    );
    let mut out = String::new();
    let _ = writeln!(out, "{}", heading.bold());
    orphans.iter().for_each(|e| {
        let _ = writeln!(out, "  {}:{}  {}", e.file, e.line, e.detail);
    });
    out
}

// ── Public entry points ────────────────────────────────────────────

/// Print the analysis as a plain-text terminal report. Uses the
/// `TextReporter` ReporterImpl to render, then writes once.
///
/// `findings_entries` is a pre-collected list of all findings (built
/// by the caller via `findings_list::collect_all_findings`) so the
/// summary's inline-location hints and the compact mode share the
/// same source. `suggestions_text` is appended after the report body
/// when `Some`.
pub fn print_text(
    analysis: &super::AnalysisResult,
    findings_entries: &[FindingEntry],
    verbose: bool,
    suggestions_text: Option<&str>,
) {
    use crate::ports::Reporter;
    let reporter = TextReporter {
        summary: &analysis.summary,
        function_analyses: &analysis.results,
        findings_entries,
        verbose,
        suggestions_text,
    };
    print!("{}", reporter.render(&analysis.findings, &analysis.data));
}

/// Print summary section. Test-helper: backward-compat for the test
/// module which still calls the per-section entries.
pub fn print_summary_only(summary: &Summary, findings: &[FindingEntry]) {
    print!("{}", format_summary_section(summary, findings));
}

/// Print only the file-grouped function listings (verbose mode).
pub fn print_files_only(results: &[FunctionAnalysis]) {
    print!("{}", format_files_section(results, true));
}

#[cfg(test)]
mod tests;
