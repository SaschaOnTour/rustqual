//! Test-Quality projection: TqWarning → typed `Vec<TqFinding>`.

use crate::adapters::analyzers::tq::{TqAnalysis, TqWarning, TqWarningKind};
use crate::domain::findings::{CoverageEvidence, TqFinding, TqFindingKind};
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
    let (rule_id, kind, uncovered_lines, coverage) = match &w.kind {
        TqWarningKind::NoAssertion => (
            "tq/no_assertion",
            TqFindingKind::NoAssertion,
            None,
            CoverageEvidence::NotApplicable,
        ),
        TqWarningKind::NoSut => (
            "tq/no_sut",
            TqFindingKind::NoSut,
            None,
            CoverageEvidence::NotApplicable,
        ),
        TqWarningKind::Untested { measured } => (
            "tq/untested",
            TqFindingKind::Untested,
            None,
            // The one kind where it varies: the other four are constant by
            // construction — TQ-004 and TQ-005 exist only when a report does.
            match measured {
                true => CoverageEvidence::Measured,
                false => CoverageEvidence::CallGraph,
            },
        ),
        TqWarningKind::Uncovered => (
            "tq/uncovered",
            TqFindingKind::Uncovered,
            None,
            CoverageEvidence::Measured,
        ),
        TqWarningKind::UntestedLogic { uncovered_lines } => (
            "tq/untested_logic",
            TqFindingKind::UntestedLogic,
            Some(uncovered_lines.clone()),
            CoverageEvidence::Measured,
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
        coverage,
    }
}
