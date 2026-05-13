//! IOSP-dimension projection: `Classification::Violation` → typed
//! `Vec<IospFinding>` with logic + call locations.

use crate::adapters::analyzers::iosp::{
    CallOccurrence, Classification, FunctionAnalysis, LogicOccurrence,
};
use crate::domain::findings::{CallLocation, IospFinding, LogicLocation};
use crate::domain::{Dimension, Finding, Severity};

const DIM: Dimension = Dimension::Iosp;
const RULE_ID: &str = "iosp/violation";

/// Project IOSP analyzer output into typed IospFinding entries.
///
/// Only `Classification::Violation` produces findings. Suppressed functions
/// are still included with `common.suppressed = true` so reporters can
/// surface the suppression ratio.
pub(crate) fn project_iosp(results: &[FunctionAnalysis]) -> Vec<IospFinding> {
    results
        .iter()
        .filter_map(|f| match &f.classification {
            Classification::Violation {
                logic_locations,
                call_locations,
                ..
            } => Some(build(f, logic_locations, call_locations)),
            _ => None,
        })
        .collect()
}

fn build(
    f: &FunctionAnalysis,
    logic_locations: &[LogicOccurrence],
    call_locations: &[CallOccurrence],
) -> IospFinding {
    IospFinding {
        common: Finding {
            file: f.file.clone(),
            line: f.line,
            column: 0,
            dimension: DIM,
            rule_id: RULE_ID.into(),
            message: format!(
                "IOSP violation in {}: logic + calls mixed",
                f.qualified_name
            ),
            severity: f.severity.clone().unwrap_or(Severity::Medium),
            suppressed: f.suppressed,
        },
        logic_locations: logic_locations
            .iter()
            .map(|l| LogicLocation {
                kind: l.kind.clone(),
                line: l.line,
            })
            .collect(),
        call_locations: call_locations
            .iter()
            .map(|c| CallLocation {
                name: c.name.clone(),
                line: c.line,
            })
            .collect(),
        effort_score: f.effort_score,
    }
}
