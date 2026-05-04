//! Projection: legacy state structures → typed `AnalysisData`.
//!
//! Sister of the `findings/*` projection adapters. Where those project
//! analyzer warnings into typed Findings, this projects analyzer state
//! (per-function classifications, per-module coupling metrics) into
//! typed records that reporters consume via `AnalysisReporter`.

use crate::adapters::analyzers::coupling::CouplingAnalysis;
use crate::adapters::analyzers::iosp::{
    Classification as LegacyClassification, ComplexityHotspot as LegacyHotspot,
    ComplexityMetrics as LegacyMetrics, FunctionAnalysis, LogicOccurrence as LegacyLogicOccurrence,
    MagicNumberOccurrence as LegacyMagicNumber,
};
use crate::domain::analysis_data::{
    ComplexityMetricsRecord, FunctionClassification, FunctionRecord, LogicOccurrenceRecord,
    MagicNumberOccurrence, ModuleCouplingRecord, NestingHotspot,
};
use crate::domain::AnalysisData;

/// Project all per-function and per-module state into typed AnalysisData.
pub(crate) fn project_data(
    results: &[FunctionAnalysis],
    coupling: Option<&CouplingAnalysis>,
) -> AnalysisData {
    AnalysisData {
        functions: results.iter().map(project_function).collect(),
        modules: project_modules(coupling),
    }
}

// qual:allow(dry) reason: "BP-008 clone-heavy conversion is intrinsic to the legacy→typed projection. FunctionAnalysis (analyzer output) and FunctionRecord (typed domain record) need full-field copy because the layers are intentionally decoupled — domain cannot import the adapter type."
fn project_function(f: &FunctionAnalysis) -> FunctionRecord {
    FunctionRecord {
        name: f.name.clone(),
        file: f.file.clone(),
        line: f.line,
        qualified_name: f.qualified_name.clone(),
        parent_type: f.parent_type.clone(),
        classification: classify(&f.classification),
        severity: f.severity.clone(),
        complexity: f.complexity.as_ref().map(project_metrics),
        parameter_count: f.parameter_count,
        own_calls: f.own_calls.clone(),
        is_trait_impl: f.is_trait_impl,
        is_test: f.is_test,
        effort_score: f.effort_score,
        suppressed: f.suppressed,
        complexity_suppressed: f.complexity_suppressed,
    }
}

// qual:allow(dry) reason: "BP-006 repetitive match mapping — this is the canonical enum-to-enum bridge between adapter and domain. A `From` impl would still need a 4-arm match on either side; relocating it doesn't reduce the lines, only their location."
fn classify(c: &LegacyClassification) -> FunctionClassification {
    match c {
        LegacyClassification::Integration => FunctionClassification::Integration,
        LegacyClassification::Operation => FunctionClassification::Operation,
        LegacyClassification::Trivial => FunctionClassification::Trivial,
        LegacyClassification::Violation { .. } => FunctionClassification::Violation,
    }
}

fn project_metrics(m: &LegacyMetrics) -> ComplexityMetricsRecord {
    ComplexityMetricsRecord {
        cognitive_complexity: m.cognitive_complexity,
        cyclomatic_complexity: m.cyclomatic_complexity,
        max_nesting: m.max_nesting,
        function_lines: m.function_lines,
        unsafe_blocks: m.unsafe_blocks,
        unwrap_count: m.unwrap_count,
        expect_count: m.expect_count,
        panic_count: m.panic_count,
        todo_count: m.todo_count,
        hotspots: m.hotspots.iter().map(project_hotspot).collect(),
        magic_numbers: m.magic_numbers.iter().map(project_magic).collect(),
        logic_occurrences: m.logic_occurrences.iter().map(project_logic).collect(),
    }
}

fn project_hotspot(h: &LegacyHotspot) -> NestingHotspot {
    NestingHotspot {
        line: h.line,
        nesting_depth: h.nesting_depth,
        construct: h.construct.clone(),
    }
}

fn project_magic(m: &LegacyMagicNumber) -> MagicNumberOccurrence {
    MagicNumberOccurrence {
        line: m.line,
        value: m.value.clone(),
    }
}

fn project_logic(l: &LegacyLogicOccurrence) -> LogicOccurrenceRecord {
    LogicOccurrenceRecord {
        line: l.line,
        kind: l.kind.clone(),
    }
}

fn project_modules(coupling: Option<&CouplingAnalysis>) -> Vec<ModuleCouplingRecord> {
    let Some(c) = coupling else {
        return Vec::new();
    };
    c.metrics
        .iter()
        .map(|m| ModuleCouplingRecord {
            module_name: m.module_name.clone(),
            afferent: m.afferent,
            efferent: m.efferent,
            instability: m.instability,
            incoming: m.incoming.clone(),
            outgoing: m.outgoing.clone(),
            suppressed: m.suppressed,
            warning: m.warning,
        })
        .collect()
}
