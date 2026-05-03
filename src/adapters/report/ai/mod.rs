//! AI-targeted output (TOON-encoded or compact JSON).

mod details;
mod format;
mod output;
mod rows;

pub use rows::{AiArchRow, AiComplexityRow, AiCouplingRow, AiDryRow, AiIospRow, AiSrpRow, AiTqRow};

pub(crate) use format::{
    format_arch_entry, format_complexity_entry, format_coupling_entry, format_dry_entry,
    format_iosp_entry, format_srp_entry, format_tq_entry,
};

use serde_json::{json, Value};

use crate::config::Config;
use crate::domain::analysis_data::{FunctionRecord, ModuleCouplingRecord};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding, SrpFinding,
    TqFinding,
};
use crate::domain::AnalysisData;
use crate::ports::reporter::{ReporterImpl, Snapshot};
use crate::ports::Reporter;
use crate::report::AnalysisResult;

/// Output format for `AiReporter`. TOON is the default human/LLM-friendly
/// compact encoding; JSON is the same envelope as a single-line compact
/// JSON string (no pretty-printing — both formats prioritise token
/// efficiency for LLM prompts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiOutputFormat {
    Toon,
    Json,
}

/// AI-targeted reporter.
pub struct AiReporter<'a> {
    pub(crate) config: &'a Config,
    pub(crate) data: &'a AnalysisData,
    pub(crate) orphan_entries: &'a [Value],
    pub(crate) format: AiOutputFormat,
}

impl<'a> AiReporter<'a> {
    fn function_name_at(&self, file: &str, line: usize) -> String {
        self.data
            .functions
            .iter()
            .find(|fr| fr.file == file && fr.line == line)
            .map(|fr| fr.qualified_name.clone())
            .unwrap_or_default()
    }
}

impl<'a> ReporterImpl for AiReporter<'a> {
    type Output = String;

    type IospView = Vec<AiIospRow>;
    type ComplexityView = Vec<AiComplexityRow>;
    type DryView = Vec<AiDryRow>;
    type SrpView = Vec<AiSrpRow>;
    type CouplingView = Vec<AiCouplingRow>;
    type TestQualityView = Vec<AiTqRow>;
    type ArchitectureView = Vec<AiArchRow>;
    type IospDataView = ();
    type ComplexityDataView = ();
    type CouplingDataView = ();

    fn build_iosp(&self, findings: &[IospFinding]) -> Vec<AiIospRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| AiIospRow {
                function_name: self.function_name_at(&f.common.file, f.common.line),
                finding: f.clone(),
            })
            .collect()
    }

    fn build_complexity(&self, findings: &[ComplexityFinding]) -> Vec<AiComplexityRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| AiComplexityRow {
                function_name: self.function_name_at(&f.common.file, f.common.line),
                finding: f.clone(),
            })
            .collect()
    }

    fn build_dry(&self, findings: &[DryFinding]) -> Vec<AiDryRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| AiDryRow {
                function_name: self.function_name_at(&f.common.file, f.common.line),
                finding: f.clone(),
            })
            .collect()
    }

    fn build_srp(&self, findings: &[SrpFinding]) -> Vec<AiSrpRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| AiSrpRow {
                function_name: self.function_name_at(&f.common.file, f.common.line),
                finding: f.clone(),
            })
            .collect()
    }

    fn build_coupling(&self, findings: &[CouplingFinding]) -> Vec<AiCouplingRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| AiCouplingRow {
                function_name: self.function_name_at(&f.common.file, f.common.line),
                finding: f.clone(),
            })
            .collect()
    }

    fn build_test_quality(&self, findings: &[TqFinding]) -> Vec<AiTqRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| AiTqRow {
                function_name: f.function_name.clone(),
                finding: f.clone(),
            })
            .collect()
    }

    fn build_architecture(&self, findings: &[ArchitectureFinding]) -> Vec<AiArchRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| AiArchRow { finding: f.clone() })
            .collect()
    }

    fn build_iosp_data(&self, _: &[FunctionRecord]) {}
    fn build_complexity_data(&self, _: &[FunctionRecord]) {}
    fn build_coupling_data(&self, _: &[ModuleCouplingRecord]) {}

    fn publish(&self, snapshot: Snapshot<Self>) -> String {
        let Snapshot {
            iosp,
            complexity,
            dry,
            srp,
            coupling,
            test_quality,
            architecture,
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
            + self.orphan_entries.len();
        let mut all_entries: Vec<Value> = Vec::with_capacity(cap);
        all_entries.extend(iosp.into_iter().map(format_iosp_entry));
        all_entries.extend(complexity.into_iter().map(format_complexity_entry));
        all_entries.extend(dry.into_iter().map(format_dry_entry));
        all_entries.extend(srp.into_iter().map(|r| format_srp_entry(r, self.config)));
        all_entries.extend(coupling.into_iter().map(format_coupling_entry));
        all_entries.extend(test_quality.into_iter().map(format_tq_entry));
        all_entries.extend(architecture.into_iter().map(format_arch_entry));
        all_entries.extend(self.orphan_entries.iter().cloned());

        let total = all_entries.len();
        let mut value = json!({
            "version": env!("CARGO_PKG_VERSION"),
            "findings": total,
        });
        if total > 0 {
            value["findings_by_file"] = output::group_by_file(all_entries);
        }
        match self.format {
            AiOutputFormat::Toon => toon_encode::encode_toon(&value, 0),
            AiOutputFormat::Json => {
                serde_json::to_string(&value).unwrap_or_else(|_| format!("{value}"))
            }
        }
    }
}

pub fn print_ai(analysis: &AnalysisResult, config: &Config) {
    let orphan_entries = output::orphan_suppression_entries(&analysis.orphan_suppressions);
    let reporter = AiReporter {
        config,
        data: &analysis.data,
        orphan_entries: &orphan_entries,
        format: AiOutputFormat::Toon,
    };
    println!("{}", reporter.render(&analysis.findings, &analysis.data));
}

pub fn print_ai_json(analysis: &AnalysisResult, config: &Config) {
    let orphan_entries = output::orphan_suppression_entries(&analysis.orphan_suppressions);
    let reporter = AiReporter {
        config,
        data: &analysis.data,
        orphan_entries: &orphan_entries,
        format: AiOutputFormat::Json,
    };
    println!("{}", reporter.render(&analysis.findings, &analysis.data));
}
