//! GitHub-annotation reporter tests, split into focused sub-files (each ≤ the
//! SRP file-length cap); shared imports + the render_*_chunk / render_github
//! helpers live here and reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};
pub(super) use crate::adapters::report::github::build::{
    build_architecture_view, build_complexity_view, build_coupling_view, build_dry_view,
    build_iosp_view, build_srp_view, build_tq_view,
};
pub(super) use crate::adapters::report::github::format::{
    format_architecture, format_complexity, format_coupling, format_dry, format_iosp, format_srp,
    format_tq,
};
pub(super) use crate::adapters::report::test_support::{make_analysis, make_result, violation};
pub(super) use crate::domain::findings::{ArchitectureFinding, IospFinding};
pub(super) use crate::domain::Finding;
pub(super) use crate::ports::Reporter;
pub(super) use crate::report::github::*;
pub(super) use crate::report::{AnalysisResult, Summary};

mod annotations;
mod messages;
mod rendering;

// Wrappers that preserve the test API: take a finding slice, return
// the formatted annotation block. They go through the new build →
// format pipeline so the tests exercise the real path.
pub(super) fn render_iosp_chunk(findings: &[IospFinding]) -> String {
    format_iosp(&build_iosp_view(findings))
}
pub(super) fn render_architecture_chunk(findings: &[ArchitectureFinding]) -> String {
    format_architecture(&build_architecture_view(findings))
}
pub(super) fn render_complexity_chunk(
    findings: &[crate::domain::findings::ComplexityFinding],
) -> String {
    format_complexity(&build_complexity_view(findings))
}
pub(super) fn render_dry_chunk(findings: &[crate::domain::findings::DryFinding]) -> String {
    format_dry(&build_dry_view(findings))
}
pub(super) fn render_srp_chunk(findings: &[crate::domain::findings::SrpFinding]) -> String {
    format_srp(&build_srp_view(findings))
}
pub(super) fn render_coupling_chunk(
    findings: &[crate::domain::findings::CouplingFinding],
) -> String {
    format_coupling(&build_coupling_view(findings))
}
pub(super) fn render_tq_chunk(findings: &[crate::domain::findings::TqFinding]) -> String {
    format_tq(&build_tq_view(findings))
}

/// Render the full GitHub-annotation output for `analysis`.
pub(super) fn render_github(analysis: &AnalysisResult) -> String {
    crate::adapters::report::github::GithubReporter {
        summary: &analysis.summary,
    }
    .render(&analysis.findings, &analysis.data)
}
