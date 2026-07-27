//! AI/TOON reporter tests, split into focused sub-files (each ≤ the SRP
//! file-length cap); shared imports + the build_ai_value/empty_analysis/
//! arch_common/make_reporter helpers live here and reach the sub-modules via
//! `use super::*`.

pub(super) use crate::adapters::analyzers::iosp::{
    CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis, LogicOccurrence,
    MagicNumberOccurrence,
};
pub(super) use crate::config::Config;
pub(super) use crate::domain::analysis_data::FunctionRecord;
pub(super) use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, ComplexityFindingKind, CouplingFinding,
    CouplingFindingDetails, CouplingFindingKind, DryFinding, DryFindingDetails, DryFindingKind,
    DuplicateParticipant, IospFinding, SrpFinding, SrpFindingDetails, SrpFindingKind, TqFinding,
    TqFindingKind,
};
pub(super) use crate::domain::Finding;
pub(super) use crate::ports::reporter::ReporterImpl;
pub(super) use crate::ports::Reporter;
pub(super) use crate::report::ai::{
    format_arch_entry, format_complexity_entry, format_coupling_entry, format_dry_entry,
    format_iosp_entry, format_srp_entry, format_tq_entry, AiOutputFormat, AiReporter,
};
pub(super) use crate::report::AnalysisResult;
pub(super) use serde_json::Value;

mod build_ai_value_shape;
mod details;
mod dimension_entries_a;
mod dimension_entries_b;
mod smoke_and_orphan;

/// Test-local helper: render via `AiReporter` with `Json` format and
/// parse back to `Value` so assertions can inspect structured data.
/// Lives here (not in production code) to keep the panic out of
/// production builds and avoid the architecture rule against loose
/// `#[cfg(test)]` items in production files.
pub(super) fn build_ai_value(analysis: &AnalysisResult, config: &Config) -> Value {
    let reporter = AiReporter {
        config,
        coverage_mode: analysis.summary.coverage_mode(),
        data: &analysis.data,
        format: AiOutputFormat::Json,
    };
    let json_str = reporter.render(&analysis.findings, &analysis.data);
    serde_json::from_str(&json_str).expect("AiReporter::render(Json) must produce valid JSON")
}

pub(super) fn empty_analysis() -> AnalysisResult {
    AnalysisResult {
        results: vec![],
        summary: crate::report::Summary::default(),
        findings: crate::domain::AnalysisFindings::default(),
        data: crate::domain::AnalysisData::default(),
    }
}

pub(super) fn arch_common(file: &str, line: usize, severity: crate::domain::Severity) -> Finding {
    Finding {
        file: file.into(),
        line,
        column: 0,
        dimension: crate::findings::Dimension::Architecture,
        rule_id: "architecture/test".into(),
        message: "test".into(),
        severity,
        suppressed: false,
    }
}

// ── AiReporter trait methods: per-dimension entry shape ────────────

pub(super) fn make_reporter<'a>(
    config: &'a Config,
    data: &'a crate::domain::AnalysisData,
) -> AiReporter<'a> {
    AiReporter {
        config,
        coverage_mode: "call-graph-only",
        data,
        format: AiOutputFormat::Json,
    }
}

/// A Violation `FunctionRecord` for the IOSP function-name-resolution tests.
pub(super) fn violation_record(file: &str, line: usize) -> FunctionRecord {
    use crate::domain::analysis_data::FunctionClassification;
    FunctionRecord {
        name: "bad_fn".into(),
        file: file.into(),
        line,
        qualified_name: "MyType::bad_fn".into(),
        parent_type: Some("MyType".into()),
        classification: FunctionClassification::Violation,
        severity: Some(crate::domain::Severity::Medium),
        complexity: None,
        parameter_count: 0,
        own_calls: vec![],
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
        suppressed: false,
        complexity_suppressed: false,
    }
}

/// An IOSP violation `Finding` at `file:line` with no logic/call locations.
pub(super) fn iosp_finding(file: &str, line: usize) -> IospFinding {
    IospFinding {
        common: Finding {
            file: file.into(),
            line,
            column: 0,
            dimension: crate::findings::Dimension::Iosp,
            rule_id: "iosp/violation".into(),
            message: "x".into(),
            severity: crate::domain::Severity::Medium,
            suppressed: false,
        },
        logic_locations: vec![],
        call_locations: vec![],
        effort_score: None,
    }
}
