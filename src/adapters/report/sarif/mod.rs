//! SARIF v2.1.0 reporter for GitHub Code Scanning integration.
//!
//! `SarifReporter` implements `ReporterImpl` over typed Findings.
//! Each per-dim `build_*` projects findings into typed `SarifResultRow`
//! views (rule_id + severity + message + location). `publish` flattens
//! the rows, appends orphan-suppression and suppression-ratio rows,
//! converts everything to SARIF JSON, and serialises.

mod rules;

use rules::sarif_rules;
use serde_json::{json, Value};

use crate::domain::analysis_data::{FunctionRecord, ModuleCouplingRecord};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, ComplexityFindingKind, CouplingFinding,
    CouplingFindingDetails, DryFinding, DryFindingDetails, DryFindingKind, IospFinding, SrpFinding,
    SrpFindingDetails, SrpFindingKind, TqFinding, TqFindingKind,
};
use crate::ports::reporter::{ReporterImpl, Snapshot};
use crate::ports::Reporter;
use crate::report::{AnalysisResult, OrphanSuppressionWarning, Summary};

/// One SARIF result, structured. Holds the borrowed finding plus the
/// SARIF-specific `rule_id` mapping; converted to a SARIF JSON Value
/// in `publish`.
pub struct SarifResultRow {
    pub(crate) rule_id: String,
    pub(crate) finding: crate::domain::Finding,
}

/// SARIF reporter. Holds the borrowed bits that `publish` needs to
/// finalise the envelope (orphan rows).
pub struct SarifReporter<'a> {
    pub(crate) summary: &'a Summary,
    pub(crate) orphan_suppressions: &'a [OrphanSuppressionWarning],
}

impl<'a> ReporterImpl for SarifReporter<'a> {
    type Output = String;

    type IospView = Vec<SarifResultRow>;
    type ComplexityView = Vec<SarifResultRow>;
    type DryView = Vec<SarifResultRow>;
    type SrpView = Vec<SarifResultRow>;
    type CouplingView = Vec<SarifResultRow>;
    type TestQualityView = Vec<SarifResultRow>;
    type ArchitectureView = Vec<SarifResultRow>;
    type IospDataView = ();
    type ComplexityDataView = ();
    type CouplingDataView = ();

    fn build_iosp(&self, findings: &[IospFinding]) -> Vec<SarifResultRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| row_from_common(&f.common, "A01"))
            .collect()
    }

    fn build_complexity(&self, findings: &[ComplexityFinding]) -> Vec<SarifResultRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| row_from_common(&f.common, complexity_rule(f.kind)))
            .collect()
    }

    fn build_dry(&self, findings: &[DryFinding]) -> Vec<SarifResultRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| row_from_common(&f.common, dry_rule(&f.kind)))
            .collect()
    }

    fn build_srp(&self, findings: &[SrpFinding]) -> Vec<SarifResultRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| row_from_common(&f.common, srp_rule(f)))
            .collect()
    }

    fn build_coupling(&self, findings: &[CouplingFinding]) -> Vec<SarifResultRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| row_from_common(&f.common, coupling_rule(f)))
            .collect()
    }

    fn build_test_quality(&self, findings: &[TqFinding]) -> Vec<SarifResultRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| row_from_common(&f.common, tq_rule(&f.kind)))
            .collect()
    }

    fn build_architecture(&self, findings: &[ArchitectureFinding]) -> Vec<SarifResultRow> {
        findings
            .iter()
            .filter(|f| !f.common.suppressed)
            .map(|f| row_from_common(&f.common, &f.common.rule_id))
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
        let chunks = [
            iosp,
            complexity,
            dry,
            srp,
            coupling,
            test_quality,
            architecture,
        ];
        let cap =
            chunks.iter().map(|c| c.len()).sum::<usize>() + self.orphan_suppressions.len() + 1;
        let mut sarif_results: Vec<Value> = Vec::with_capacity(cap);
        for chunk in chunks {
            sarif_results.extend(chunk.into_iter().map(row_to_sarif_value));
        }
        sarif_results.extend(orphan_suppression_results(self.orphan_suppressions));
        sarif_results.extend(suppression_ratio_result(self.summary));
        let rules = sarif_rules();
        let envelope = json!({
            "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
            "version": "2.1.0",
            "runs": [{
                "tool": {
                    "driver": {
                        "name": "rustqual",
                        "informationUri": "https://github.com/DEIN-USERNAME/rustqual",
                        "rules": rules,
                    }
                },
                "results": sarif_results,
            }]
        });
        serde_json::to_string_pretty(&envelope)
            .unwrap_or_else(|e| format!("{{\"error\":\"SARIF serialization failed: {e}\"}}"))
    }
}

// ── Row construction ────────────────────────────────────────────────

fn row_from_common(common: &crate::domain::Finding, rule_id: &str) -> SarifResultRow {
    SarifResultRow {
        rule_id: rule_id.to_string(),
        finding: common.clone(),
    }
}

fn row_to_sarif_value(r: SarifResultRow) -> Value {
    let level = r.finding.severity.levels().sarif;
    if r.finding.file.is_empty() {
        json!({
            "ruleId": r.rule_id,
            "level": level,
            "message": { "text": r.finding.message },
            "locations": []
        })
    } else {
        json!({
            "ruleId": r.rule_id,
            "level": level,
            "message": { "text": r.finding.message },
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": { "uri": r.finding.file },
                    "region": { "startLine": r.finding.line }
                }
            }]
        })
    }
}

// ── Rule-id mapping per dimension ───────────────────────────────────

fn complexity_rule(kind: ComplexityFindingKind) -> &'static str {
    match kind {
        ComplexityFindingKind::Cognitive => "CX-001",
        ComplexityFindingKind::Cyclomatic => "CX-002",
        ComplexityFindingKind::MagicNumber => "CX-003",
        ComplexityFindingKind::FunctionLength => "CX-004",
        ComplexityFindingKind::NestingDepth => "CX-005",
        ComplexityFindingKind::Unsafe => "CX-006",
        ComplexityFindingKind::ErrorHandling => "A20",
    }
}

fn dry_rule(kind: &DryFindingKind) -> &'static str {
    match kind {
        DryFindingKind::DuplicateExact | DryFindingKind::DuplicateSimilar => "DRY-001",
        DryFindingKind::DeadCodeUncalled | DryFindingKind::DeadCodeTestOnly => "DRY-002",
        DryFindingKind::Fragment => "DRY-003",
        DryFindingKind::Boilerplate => "DRY-004",
        DryFindingKind::RepeatedMatch => "DRY-005",
        DryFindingKind::Wildcard => "DRY-006",
    }
}

fn srp_rule(f: &SrpFinding) -> &'static str {
    match (&f.kind, &f.details) {
        (SrpFindingKind::StructCohesion, _) => "SRP-001",
        (SrpFindingKind::ModuleLength, _) => "SRP-002",
        (SrpFindingKind::ParameterCount, _) => "SRP-003",
        (SrpFindingKind::Structural, SrpFindingDetails::Structural { code, .. }) => {
            structural_rule(code)
        }
        _ => "SRP-001",
    }
}

fn coupling_rule(f: &CouplingFinding) -> &'static str {
    match &f.details {
        CouplingFindingDetails::Cycle { .. } => "CP-001",
        CouplingFindingDetails::SdpViolation { .. } => "CP-002",
        CouplingFindingDetails::ThresholdExceeded { .. } => "CP-003",
        CouplingFindingDetails::Structural { code, .. } => structural_rule(code),
    }
}

fn structural_rule(code: &str) -> &'static str {
    match code {
        "BTC" => "ST-BTC",
        "SLM" => "ST-SLM",
        "NMS" => "ST-NMS",
        "OI" => "ST-OI",
        "SIT" => "ST-SIT",
        "DEH" => "ST-DEH",
        "IET" => "ST-IET",
        _ => "ST-UNKNOWN",
    }
}

fn tq_rule(kind: &TqFindingKind) -> &'static str {
    kind.meta().sarif_rule
}

// ── Orphan + suppression-ratio rows (extra results, not findings) ───

fn orphan_suppression_results(orphans: &[OrphanSuppressionWarning]) -> Vec<Value> {
    orphans
        .iter()
        .map(|w| {
            let dims: String = if w.dimensions.is_empty() {
                "all dims (wildcard)".to_string()
            } else {
                w.dimensions
                    .iter()
                    .map(|d| format!("{d}"))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let message = match &w.reason {
                Some(r) => format!(
                    "Stale qual:allow({dims}) marker — no finding in window. Reason was: {r}"
                ),
                None => format!("Stale qual:allow({dims}) marker — no finding in window."),
            };
            json!({
                "ruleId": "ORPHAN-001",
                "level": "warning",
                "message": { "text": message },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": w.file },
                        "region": { "startLine": w.line }
                    }
                }]
            })
        })
        .collect()
}

fn suppression_ratio_result(summary: &Summary) -> Vec<Value> {
    if !summary.suppression_ratio_exceeded {
        return vec![];
    }
    vec![json!({
        "ruleId": "SUP-001",
        "level": "note",
        "message": {
            "text": format!(
                "Suppression ratio exceeded: {} suppressions (qual:allow + #[allow]) of {} functions",
                summary.all_suppressions, summary.total,
            )
        },
        "locations": []
    })]
}

// ── Public entry points ─────────────────────────────────────────────

/// Print results in SARIF v2.1.0 format for GitHub Code Scanning integration.
pub fn print_sarif(analysis: &AnalysisResult) {
    println!("{}", build_sarif_string(analysis));
}

/// Build the SARIF v2.1.0 JSON string from an analysis result.
pub fn build_sarif_string(analysis: &AnalysisResult) -> String {
    let reporter = SarifReporter {
        summary: &analysis.summary,
        orphan_suppressions: &analysis.orphan_suppressions,
    };
    reporter.render(&analysis.findings, &analysis.data)
}

// qual:test_helper
/// Build the SARIF v2.1.0 JSON value from an analysis result.
/// Convenience wrapper for tests; production callers use
/// `build_sarif_string` or `print_sarif`.
pub fn build_sarif_value(analysis: &AnalysisResult) -> Value {
    serde_json::from_str(&build_sarif_string(analysis))
        .unwrap_or_else(|e| json!({ "error": format!("SARIF parse failed: {e}") }))
}

#[cfg(test)]
mod tests;
