//! `JsonReporter` — implements `ReporterImpl` over the typed Findings +
//! Data payload. Each `build_*` returns a `JsonChunk` populated with
//! only the sections that dimension contributes; `publish` merges them
//! into the final `JsonOutput` envelope and serialises.

use super::chunk::JsonChunk;
use super::dry::{
    build_boilerplate, build_dead_code, build_duplicates, build_fragments, build_repeated_matches,
    build_wildcards,
};
use super::functions::build_functions;
use super::misc::{build_architecture, build_tq};
use super::srp_coupling::{
    build_coupling_modules, build_cycles, build_sdp_violations, build_srp_lists, build_structural,
};
use crate::domain::analysis_data::{FunctionRecord, ModuleCouplingRecord};
use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding, SrpFinding,
    TqFinding,
};
use crate::domain::AnalysisFindings;
use crate::ports::reporter::{ReporterImpl, Snapshot};
use crate::report::json::JsonOutputComposer;

pub struct JsonReporter<'a> {
    /// Borrowed reference to the raw findings used by `build_iosp_data`
    /// to blend IOSP violation locations into per-function entries.
    pub(crate) findings: &'a AnalysisFindings,
    /// Composer that wraps the merged `JsonChunk` into the public
    /// `JsonOutput` envelope, including summary and orphan-suppression
    /// fields the chunk itself doesn't carry. Held by the reporter so
    /// `publish` can finalise without needing `&AnalysisResult` access.
    pub(crate) composer: &'a JsonOutputComposer<'a>,
}

impl<'a> ReporterImpl for JsonReporter<'a> {
    type Output = String;

    type IospView = JsonChunk;
    type ComplexityView = JsonChunk;
    type DryView = JsonChunk;
    type SrpView = JsonChunk;
    type CouplingView = JsonChunk;
    type TestQualityView = JsonChunk;
    type ArchitectureView = JsonChunk;
    type IospDataView = JsonChunk;
    type ComplexityDataView = JsonChunk;
    type CouplingDataView = JsonChunk;

    fn build_iosp(&self, _: &[IospFinding]) -> JsonChunk {
        // IOSP findings are merged into per-function entries built in
        // `build_iosp_data` (which has the `FunctionRecord` slice).
        JsonChunk::default()
    }

    fn build_complexity(&self, _: &[ComplexityFinding]) -> JsonChunk {
        // Complexity warnings are reflected in per-function entries
        // built in `build_iosp_data` via `FunctionRecord.complexity`.
        JsonChunk::default()
    }

    fn build_dry(&self, findings: &[DryFinding]) -> JsonChunk {
        JsonChunk {
            duplicates: build_duplicates(findings),
            dead_code: build_dead_code(findings),
            fragments: build_fragments(findings),
            wildcards: build_wildcards(findings),
            boilerplate: build_boilerplate(findings),
            repeated_matches: build_repeated_matches(findings),
            ..Default::default()
        }
    }

    fn build_srp(&self, findings: &[SrpFinding]) -> JsonChunk {
        let (struct_warnings, module_warnings, param_warnings) = build_srp_lists(findings);
        JsonChunk {
            srp_struct: struct_warnings,
            srp_module: module_warnings,
            srp_param: param_warnings,
            structural: build_structural(findings, &[]),
            ..Default::default()
        }
    }

    fn build_coupling(&self, findings: &[CouplingFinding]) -> JsonChunk {
        JsonChunk {
            cycles: build_cycles(findings),
            sdp_violations: build_sdp_violations(findings),
            structural: build_structural(&[], findings),
            ..Default::default()
        }
    }

    fn build_test_quality(&self, findings: &[TqFinding]) -> JsonChunk {
        JsonChunk {
            tq_warnings: build_tq(findings),
            ..Default::default()
        }
    }

    fn build_architecture(&self, findings: &[ArchitectureFinding]) -> JsonChunk {
        JsonChunk {
            architecture: build_architecture(findings),
            ..Default::default()
        }
    }

    fn build_iosp_data(&self, functions: &[FunctionRecord]) -> JsonChunk {
        // Per-function entries blend FunctionRecord state + IospFinding
        // violation locations. Looked up across both payloads.
        JsonChunk {
            functions: build_functions(functions, &self.findings.iosp),
            ..Default::default()
        }
    }

    fn build_complexity_data(&self, _: &[FunctionRecord]) -> JsonChunk {
        // Complexity metrics are inlined into JsonFunction by
        // build_iosp_data via FunctionRecord.complexity.
        JsonChunk::default()
    }

    fn build_coupling_data(&self, modules: &[ModuleCouplingRecord]) -> JsonChunk {
        JsonChunk {
            coupling_modules: build_coupling_modules(modules),
            ..Default::default()
        }
    }

    fn publish(&self, snapshot: Snapshot<Self>) -> String {
        let Snapshot {
            iosp,
            complexity,
            dry,
            srp,
            coupling,
            test_quality,
            architecture,
            iosp_data,
            complexity_data,
            coupling_data,
        } = snapshot;
        let mut merged = JsonChunk::default();
        for chunk in [
            iosp,
            complexity,
            dry,
            srp,
            coupling,
            test_quality,
            architecture,
            iosp_data,
            complexity_data,
            coupling_data,
        ] {
            merged.extend_from(chunk);
        }
        self.composer.compose(merged)
    }
}
