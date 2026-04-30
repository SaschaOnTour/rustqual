//! Project typed Findings into per-dim view rows.

use super::views::{
    GithubArchitectureRow, GithubArchitectureView, GithubComplexityRow, GithubComplexityView,
    GithubCouplingView, GithubDetailListView, GithubDetailRow, GithubDryView, GithubIospRow,
    GithubIospView, GithubSrpView, GithubTqRow, GithubTqView,
};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding, SrpFinding,
    TqFinding,
};
use crate::domain::Finding;

pub(crate) fn build_iosp_view(findings: &[IospFinding]) -> GithubIospView {
    GithubIospView {
        rows: findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| GithubIospRow {
                file: f.common.file.clone(),
                line: f.common.line,
                severity: f.common.severity.clone(),
                logic_locations: f
                    .logic_locations
                    .iter()
                    .map(|l| (l.kind.clone(), l.line))
                    .collect(),
                call_locations: f
                    .call_locations
                    .iter()
                    .map(|c| (c.name.clone(), c.line))
                    .collect(),
                effort_score: f.effort_score,
            })
            .collect(),
    }
}

pub(crate) fn build_complexity_view(findings: &[ComplexityFinding]) -> GithubComplexityView {
    GithubComplexityView {
        rows: findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| GithubComplexityRow {
                file: f.common.file.clone(),
                line: f.common.line,
                kind: f.kind,
                message: f.common.message.clone(),
            })
            .collect(),
    }
}

pub(crate) fn build_dry_view(findings: &[DryFinding]) -> GithubDryView {
    build_detail_view(findings, |f| &f.common, |f| f.details.clone())
}

pub(crate) fn build_srp_view(findings: &[SrpFinding]) -> GithubSrpView {
    build_detail_view(findings, |f| &f.common, |f| f.details.clone())
}

pub(crate) fn build_coupling_view(findings: &[CouplingFinding]) -> GithubCouplingView {
    build_detail_view(findings, |f| &f.common, |f| f.details.clone())
}

pub(crate) fn build_tq_view(findings: &[TqFinding]) -> GithubTqView {
    GithubTqView {
        rows: findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| GithubTqRow {
                file: f.common.file.clone(),
                line: f.common.line,
                severity: f.common.severity.clone(),
                message: f.common.message.clone(),
            })
            .collect(),
    }
}

pub(crate) fn build_architecture_view(findings: &[ArchitectureFinding]) -> GithubArchitectureView {
    GithubArchitectureView {
        rows: findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| GithubArchitectureRow {
                file: f.common.file.clone(),
                line: f.common.line,
                severity: f.common.severity.clone(),
                rule_id: f.common.rule_id.clone(),
                message: f.common.message.clone(),
            })
            .collect(),
    }
}

fn build_detail_view<F, D>(
    findings: &[F],
    common: impl Fn(&F) -> &Finding,
    details: impl Fn(&F) -> D,
) -> GithubDetailListView<D> {
    GithubDetailListView {
        rows: findings
            .iter()
            .filter(|f| !common(f).suppressed)
            .map(|f| {
                let c = common(f);
                GithubDetailRow {
                    file: c.file.clone(),
                    line: c.line,
                    severity: c.severity.clone(),
                    details: details(f),
                    fallback_message: c.message.clone(),
                }
            })
            .collect(),
    }
}
