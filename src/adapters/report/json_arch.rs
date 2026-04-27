//! Architecture-dimension projection for the JSON output.
//!
//! Mirrors the orphan-suppression / structural / tq sub-modules: a
//! single mapping fn that consumes `architecture_findings` from
//! `AnalysisResult` and produces the JSON-shaped `Vec<JsonArchitectureFinding>`.

use super::json_types::JsonArchitectureFinding;
use crate::domain::{Finding, Severity};

/// Project the architecture-dimension findings for JSON output.
/// Operation: per-finding field copy + severity stringify.
pub(crate) fn build_json_arch(findings: &[Finding]) -> Vec<JsonArchitectureFinding> {
    findings
        .iter()
        .map(|f| JsonArchitectureFinding {
            file: f.file.clone(),
            line: f.line,
            rule_id: f.rule_id.clone(),
            severity: severity_str(&f.severity).to_string(),
            message: f.message.clone(),
            suppressed: f.suppressed,
        })
        .collect()
}

/// Stringify `Severity` for JSON consumers.
/// Operation: variant dispatch.
fn severity_str(severity: &Severity) -> &'static str {
    match severity {
        Severity::Low => "low",
        Severity::Medium => "medium",
        Severity::High => "high",
    }
}
