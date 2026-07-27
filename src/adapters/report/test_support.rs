//! Shared fixtures for the report-reporter unit tests.
//!
//! Every reporter test (json, github, baseline, root, suggestions, html,
//! text, sarif, …) needs the same two builders: a single `FunctionAnalysis`
//! with sane defaults and an `AnalysisResult` wrapping a set of results with
//! the standard summary / projection. Keeping them here means one source of
//! truth, reachable from every `report::*::tests` module, instead of a copy
//! per test file.

use crate::adapters::analyzers::iosp::{
    compute_severity, CallOccurrence, Classification, FunctionAnalysis, LogicOccurrence,
};
use crate::domain::findings::CoverageEvidence;
use crate::report::{AnalysisResult, Summary};

/// A `Violation` classification with one logic location (`logic_kind` at
/// line 1) and one own-call location (`call_name` at line 2) — the shape
/// the reporter tests use to exercise violation rendering.
pub(crate) fn violation(logic_kind: &str, call_name: &str) -> Classification {
    Classification::Violation {
        has_logic: true,
        has_own_calls: true,
        logic_locations: vec![LogicOccurrence {
            kind: logic_kind.into(),
            line: 1,
        }],
        call_locations: vec![CallOccurrence {
            name: call_name.into(),
            line: 2,
        }],
    }
}

/// A `FunctionAnalysis` for `name` with the given classification and all
/// warning/metric fields defaulted (file `test.rs`, line 1, no complexity).
pub(crate) fn make_result(name: &str, classification: Classification) -> FunctionAnalysis {
    let severity = compute_severity(&classification);
    FunctionAnalysis {
        name: name.to_string(),
        file: "test.rs".to_string(),
        line: 1,
        classification,
        suppressed: false,
        complexity: None,
        qualified_name: name.to_string(),
        severity,
        cognitive_warning: false,
        cyclomatic_warning: false,
        nesting_depth_warning: false,
        function_length_warning: false,
        unsafe_warning: false,
        error_handling_warning: false,
        complexity_suppressed: false,
        own_calls: vec![],
        parameter_count: 0,
        parent_type: None,
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
    }
}

/// Wrap `results` in an `AnalysisResult` with the standard summary and the
/// IOSP projection populated (other finding dimensions left empty).
pub(crate) fn make_analysis(results: Vec<FunctionAnalysis>) -> AnalysisResult {
    let summary = Summary::from_results(&results);
    let data = crate::app::projection::project_data(&results, None);
    let findings = crate::domain::AnalysisFindings {
        iosp: crate::app::projection::project_iosp(&results),
        ..Default::default()
    };
    AnalysisResult {
        results,
        summary,
        findings,
        data,
    }
}

/// The coverage evidence a TQ kind carries on its own, for fixtures that build
/// a `TqFinding` by hand.
///
/// Four of the five kinds are constant by construction — TQ-004 and TQ-005
/// exist only when a coverage report does, TQ-001 and TQ-002 never consult
/// one. `Untested` is the one kind that varies with what the report named,
/// and the kind alone cannot say which case it is: this helper answers
/// `CallGraph` for it, the answer without a report. **A fixture that needs a
/// measured `Untested` finding sets `coverage` itself** — none does today, so
/// the helper takes no second argument the call sites would all ignore.
///
/// Hardcoding `NotApplicable` everywhere made fixtures that render `[n/a]` on
/// a finding that can only come from measurement.
/// Operation: kind dispatch, no own calls.
pub(crate) fn evidence_for(kind: crate::domain::findings::TqFindingKind) -> CoverageEvidence {
    use crate::domain::findings::TqFindingKind as K;
    match kind {
        K::NoAssertion | K::NoSut => CoverageEvidence::NotApplicable,
        K::Uncovered | K::UntestedLogic => CoverageEvidence::Measured,
        K::Untested => CoverageEvidence::CallGraph,
    }
}
