//! GitHub Actions workflow-annotation reporter.

pub(crate) mod build;
pub(crate) mod format;
mod views;

use build::{
    build_architecture_view, build_complexity_view, build_coupling_view, build_dry_view,
    build_iosp_view, build_srp_view, build_tq_view,
};
use format::{
    format_architecture, format_complexity, format_coupling, format_dry, format_iosp, format_srp,
    format_tq,
};
use views::{
    GithubArchitectureView, GithubComplexityView, GithubCouplingView, GithubDryView,
    GithubIospView, GithubSrpView, GithubTqView,
};

use crate::domain::analysis_data::{FunctionRecord, ModuleCouplingRecord};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding, SrpFinding,
    TqFinding,
};
use crate::ports::reporter::{ReporterImpl, Snapshot};
use crate::ports::Reporter;
use crate::report::{AnalysisResult, OrphanSuppressionWarning, Summary};

/// GitHub Actions reporter — produces `::level file=,line=::message`
/// annotations plus a trailing summary annotation.
pub struct GithubReporter<'a> {
    pub(crate) summary: &'a Summary,
    pub(crate) orphan_suppressions: &'a [OrphanSuppressionWarning],
}

impl<'a> ReporterImpl for GithubReporter<'a> {
    type Output = String;

    type IospView = GithubIospView;
    type ComplexityView = GithubComplexityView;
    type DryView = GithubDryView;
    type SrpView = GithubSrpView;
    type CouplingView = GithubCouplingView;
    type TestQualityView = GithubTqView;
    type ArchitectureView = GithubArchitectureView;
    type IospDataView = ();
    type ComplexityDataView = ();
    type CouplingDataView = ();

    fn build_iosp(&self, findings: &[IospFinding]) -> GithubIospView {
        build_iosp_view(findings)
    }
    fn build_complexity(&self, findings: &[ComplexityFinding]) -> GithubComplexityView {
        build_complexity_view(findings)
    }
    fn build_dry(&self, findings: &[DryFinding]) -> GithubDryView {
        build_dry_view(findings)
    }
    fn build_srp(&self, findings: &[SrpFinding]) -> GithubSrpView {
        build_srp_view(findings)
    }
    fn build_coupling(&self, findings: &[CouplingFinding]) -> GithubCouplingView {
        build_coupling_view(findings)
    }
    fn build_test_quality(&self, findings: &[TqFinding]) -> GithubTqView {
        build_tq_view(findings)
    }
    fn build_architecture(&self, findings: &[ArchitectureFinding]) -> GithubArchitectureView {
        build_architecture_view(findings)
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
        let mut out = String::new();
        out.push_str(&format_iosp(&iosp));
        out.push_str(&format_complexity(&complexity));
        out.push_str(&format_dry(&dry));
        out.push_str(&format_srp(&srp));
        out.push_str(&format_coupling(&coupling));
        out.push_str(&format_tq(&test_quality));
        out.push_str(&format_architecture(&architecture));
        out.push_str(&format::format_orphan_suppressions(
            self.orphan_suppressions,
        ));
        out.push_str(&render_summary_annotation(self.summary));
        out
    }
}

/// Render the trailing summary annotation: `::error` whenever the run
/// has any finding (matches the default-fail criterion in `lib::run`);
/// `::notice` only on a clean run. Plus a `::warning` if the
/// suppression ratio threshold is exceeded.
pub fn render_summary_annotation(summary: &Summary) -> String {
    let mut out = String::new();
    if summary.suppression_ratio_exceeded {
        out.push_str(&format!(
            "::warning::Suppression ratio exceeds configured maximum ({} suppressions across {} functions)\n",
            summary.all_suppressions, summary.total,
        ));
    }
    let total = summary.total_findings();
    if total > 0 {
        out.push_str(&format!(
            "::error::Quality analysis: {total} finding(s) ({} IOSP violation(s)), {:.1}% quality score\n",
            summary.violations,
            summary.quality_score * crate::domain::PERCENTAGE_MULTIPLIER,
        ));
    } else {
        out.push_str(&format!(
            "::notice::Quality score: {:.1}% ({} functions analyzed)\n",
            summary.quality_score * crate::domain::PERCENTAGE_MULTIPLIER,
            summary.total,
        ));
    }
    out
}

/// Print results as GitHub Actions workflow annotations.
pub fn print_github(analysis: &AnalysisResult) {
    let reporter = GithubReporter {
        summary: &analysis.summary,
        orphan_suppressions: &analysis.orphan_suppressions,
    };
    print!("{}", reporter.render(&analysis.findings, &analysis.data));
}
