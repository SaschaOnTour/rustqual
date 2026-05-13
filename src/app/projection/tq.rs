//! Test-Quality projection: TqWarning → typed `Vec<TqFinding>`.

use crate::adapters::analyzers::tq::{TqAnalysis, TqWarning, TqWarningKind};
use crate::domain::findings::{TqFinding, TqFindingKind};
use crate::domain::{Dimension, Finding, Severity};

const DIM: Dimension = Dimension::TestQuality;
const SEV: Severity = Severity::Medium;

/// Project TQ analyzer output into typed TqFinding entries.
pub(crate) fn project_tq(tq: Option<&TqAnalysis>) -> Vec<TqFinding> {
    let Some(tq) = tq else {
        return Vec::new();
    };
    tq.warnings.iter().map(project_warning).collect()
}

fn project_warning(w: &TqWarning) -> TqFinding {
    let (rule_id, kind, uncovered_lines) = match &w.kind {
        TqWarningKind::NoAssertion => ("tq/no_assertion", TqFindingKind::NoAssertion, None),
        TqWarningKind::NoSut => ("tq/no_sut", TqFindingKind::NoSut, None),
        TqWarningKind::Untested => ("tq/untested", TqFindingKind::Untested, None),
        TqWarningKind::Uncovered => ("tq/uncovered", TqFindingKind::Uncovered, None),
        TqWarningKind::UntestedLogic { uncovered_lines } => (
            "tq/untested_logic",
            TqFindingKind::UntestedLogic,
            Some(uncovered_lines.clone()),
        ),
    };
    TqFinding {
        common: Finding {
            file: w.file.clone(),
            line: w.line,
            column: 0,
            dimension: DIM,
            rule_id: rule_id.into(),
            message: format!("{}: {}", rule_id, w.function_name),
            severity: SEV,
            suppressed: w.suppressed,
        },
        kind,
        function_name: w.function_name.clone(),
        uncovered_lines,
    }
}
