//! Findings-list reporter: one line per finding, used by `--findings`
//! and the text-compact summary.
//!
//! Pure-data Views: each per-dim `build_*` projects findings into a
//! per-dim row type holding the raw structured data (no formatted
//! strings). `publish` converts the rows to the public `FindingEntry`
//! shape via `format::*`, which does the category-label and
//! detail-text formatting from the row's fields.

mod categories;
mod format;
mod views;

use format::{
    format_architecture, format_complexity, format_coupling, format_dry, format_iosp, format_srp,
    format_tq,
};
use views::{
    ListArchRow, ListComplexityRow, ListCouplingRow, ListDryRow, ListIospRow, ListSrpRow, ListTqRow,
};

use crate::domain::analysis_data::{FunctionRecord, ModuleCouplingRecord};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding,
    OrphanSuppression, SrpFinding, TqFinding,
};
use crate::domain::AnalysisData;
use crate::ports::reporter::{ReporterImpl, Snapshot};
use crate::ports::Reporter;
use crate::report::AnalysisResult;

/// One row of the unified findings list — the public Output type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingEntry {
    pub file: String,
    pub line: usize,
    pub category: &'static str,
    pub detail: String,
    pub function_name: String,
}

impl FindingEntry {
    pub(crate) fn new(
        file: &str,
        line: usize,
        category: &'static str,
        detail: String,
        function_name: String,
    ) -> Self {
        Self {
            file: file.to_string(),
            line,
            category,
            detail,
            function_name,
        }
    }
}

/// Findings-list reporter. Function-name lookup is via the per-file:line
/// match against `AnalysisData.functions`. Orphan-suppression entries
/// flow through the trait via `build_orphans` → `Snapshot::orphans` →
/// `publish` — no struct-field bypass.
pub struct FindingsListReporter<'a> {
    pub(crate) data: &'a AnalysisData,
}

impl<'a> FindingsListReporter<'a> {
    fn function_name_at(&self, file: &str, line: usize) -> String {
        self.data
            .functions
            .iter()
            .find(|fr: &&FunctionRecord| fr.file == file && fr.line == line)
            .map(|fr| fr.qualified_name.clone())
            .unwrap_or_default()
    }
}

impl<'a> ReporterImpl for FindingsListReporter<'a> {
    type Output = Vec<FindingEntry>;

    type IospView = Vec<ListIospRow>;
    type ComplexityView = Vec<ListComplexityRow>;
    type DryView = Vec<ListDryRow>;
    type SrpView = Vec<ListSrpRow>;
    type CouplingView = Vec<ListCouplingRow>;
    type TestQualityView = Vec<ListTqRow>;
    type ArchitectureView = Vec<ListArchRow>;
    type OrphanView = Vec<FindingEntry>;
    type IospDataView = ();
    type ComplexityDataView = ();
    type CouplingDataView = ();

    fn build_iosp(&self, findings: &[IospFinding]) -> Vec<ListIospRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| ListIospRow {
                file: f.common.file.clone(),
                line: f.common.line,
                function_name: self.function_name_at(&f.common.file, f.common.line),
            })
            .collect()
    }

    fn build_complexity(&self, findings: &[ComplexityFinding]) -> Vec<ListComplexityRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| ListComplexityRow {
                file: f.common.file.clone(),
                line: f.common.line,
                function_name: self.function_name_at(&f.common.file, f.common.line),
                finding: f.clone(),
            })
            .collect()
    }

    fn build_dry(&self, findings: &[DryFinding]) -> Vec<ListDryRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| ListDryRow {
                file: f.common.file.clone(),
                line: f.common.line,
                function_name: self.function_name_at(&f.common.file, f.common.line),
                finding: f.clone(),
            })
            .collect()
    }

    fn build_srp(&self, findings: &[SrpFinding]) -> Vec<ListSrpRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| ListSrpRow {
                file: f.common.file.clone(),
                line: f.common.line,
                function_name: self.function_name_at(&f.common.file, f.common.line),
                finding: f.clone(),
            })
            .collect()
    }

    fn build_coupling(&self, findings: &[CouplingFinding]) -> Vec<ListCouplingRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| ListCouplingRow {
                file: f.common.file.clone(),
                line: f.common.line,
                function_name: self.function_name_at(&f.common.file, f.common.line),
                finding: f.clone(),
            })
            .collect()
    }

    fn build_test_quality(&self, findings: &[TqFinding]) -> Vec<ListTqRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| ListTqRow {
                file: f.common.file.clone(),
                line: f.common.line,
                function_name: f.function_name.clone(),
                kind: f.kind,
            })
            .collect()
    }

    fn build_architecture(&self, findings: &[ArchitectureFinding]) -> Vec<ListArchRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| ListArchRow {
                file: f.common.file.clone(),
                line: f.common.line,
                message: f.common.message.clone(),
            })
            .collect()
    }

    fn build_orphans(&self, suppressions: &[OrphanSuppression]) -> Vec<FindingEntry> {
        suppressions.iter().map(orphan_to_finding_entry).collect()
    }
    fn build_iosp_data(&self, _: &[FunctionRecord]) {}
    fn build_complexity_data(&self, _: &[FunctionRecord]) {}
    fn build_coupling_data(&self, _: &[ModuleCouplingRecord]) {}

    fn publish(&self, snapshot: Snapshot<Self>) -> Vec<FindingEntry> {
        let Snapshot {
            iosp,
            complexity,
            dry,
            srp,
            coupling,
            test_quality,
            architecture,
            orphans,
            iosp_data: (),
            complexity_data: (),
            coupling_data: (),
        } = snapshot;
        let cap = iosp.len()
            + complexity.len()
            + dry.len()
            + srp.len()
            + coupling.len()
            + test_quality.len()
            + architecture.len()
            + orphans.len();
        let mut entries: Vec<FindingEntry> = Vec::with_capacity(cap);
        entries.extend(iosp.into_iter().map(format_iosp));
        entries.extend(complexity.into_iter().map(format_complexity));
        entries.extend(dry.into_iter().map(format_dry));
        entries.extend(srp.into_iter().map(format_srp));
        entries.extend(coupling.into_iter().map(format_coupling));
        entries.extend(test_quality.into_iter().map(format_tq));
        entries.extend(architecture.into_iter().map(format_architecture));
        entries.extend(orphans);
        entries.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        entries
    }
}

/// Collect all findings as a flat Vec<FindingEntry>, sorted by file
/// then line. Orphan-suppression warnings flow through the trait via
/// `Snapshot::orphans` (populated from `AnalysisFindings.orphan_suppressions`).
pub fn collect_all_findings(analysis: &AnalysisResult) -> Vec<FindingEntry> {
    let reporter = FindingsListReporter {
        data: &analysis.data,
    };
    reporter.render(&analysis.findings, &analysis.data)
}

/// Convert one `OrphanSuppression` finding into a `FindingEntry`.
/// Per-reporter view discretion: each reporter that renders orphans
/// owns its conversion. Operation: pure data shaping, no own calls.
pub(crate) fn orphan_to_finding_entry(w: &OrphanSuppression) -> FindingEntry {
    let dims: Vec<String> = w.dimensions.iter().map(|d| d.to_string()).collect();
    let scope = if dims.is_empty() {
        "<all>".to_string()
    } else {
        dims.join(",")
    };
    let detail = match &w.reason {
        Some(r) => format!("stale qual:allow({scope}) — {r}"),
        None => format!("stale qual:allow({scope})"),
    };
    FindingEntry::new(&w.file, w.line, "ORPHAN_SUPPRESSION", detail, String::new())
}

/// Print findings in one-line-per-finding format with heading.
pub fn print_findings(entries: &[FindingEntry]) {
    if entries.is_empty() {
        return;
    }
    let n = entries.len();
    let heading = format!("═══ {} Finding{} ═══", n, if n == 1 { "" } else { "s" });
    println!("\n{}", colored::Colorize::bold(heading.as_str()));
    entries.iter().for_each(|e| {
        let detail = if e.function_name.is_empty() {
            e.detail.clone()
        } else if e.detail.is_empty() {
            format!("in {}", e.function_name)
        } else {
            format!("{}  in {}", e.detail, e.function_name)
        };
        if e.file.is_empty() {
            println!("  {}  {}", e.category, detail);
        } else {
            println!("  {}:{}  {}  {}", e.file, e.line, e.category, detail);
        }
    });
}
