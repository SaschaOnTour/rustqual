//! JSON reporter — machine-readable output for CI integration and
//! downstream tooling.
//!
//! Implements `ReporterImpl` over typed Findings + Data. Each per-dim
//! `build_*` method returns a `JsonChunk` populating its sections;
//! `publish` merges all chunks and hands them to a [`JsonOutputComposer`]
//! which wraps them in the public `JsonOutput` envelope.
//!
//! `JsonOutputComposer` lives outside the trait dispatch so the
//! summary + orphan-suppression helpers it calls remain visible to the
//! call-graph analyzer (see `project_dead_code_analyzer_blanket_dispatch_blind_spot`
//! memory entry — Trait-blanket dispatch is opaque to DRY-002).

mod chunk;
mod dry;
mod functions;
mod misc;
mod reporter;
mod srp_coupling;

use chunk::JsonChunk;
use reporter::JsonReporter;

use super::json_types::JsonOutput;
use super::AnalysisResult;
use crate::ports::Reporter;
use crate::report::Summary;

/// Wraps a merged `JsonChunk` into the public `JsonOutput` envelope.
/// Holds the summary slice; orphan suppressions arrive via the
/// trait-driven `Snapshot::orphans` view (passed to `compose`).
pub(crate) struct JsonOutputComposer<'a> {
    pub summary: &'a Summary,
}

impl<'a> JsonOutputComposer<'a> {
    pub(crate) fn compose(
        &self,
        merged: JsonChunk,
        orphan_suppressions: Vec<crate::adapters::report::json_types::JsonOrphanSuppression>,
    ) -> String {
        let output = JsonOutput {
            summary: misc::build_summary(self.summary),
            functions: merged.functions,
            coupling: if merged.coupling_modules.is_empty() && merged.cycles.is_empty() {
                None
            } else {
                Some(super::json_types::JsonCoupling {
                    modules: merged.coupling_modules,
                    cycles: merged.cycles,
                    sdp_violations: merged.sdp_violations,
                })
            },
            duplicates: merged.duplicates,
            dead_code: merged.dead_code,
            fragments: merged.fragments,
            wildcard_warnings: merged.wildcards,
            boilerplate: merged.boilerplate,
            tq_warnings: merged.tq_warnings,
            structural_warnings: merged.structural,
            repeated_matches: merged.repeated_matches,
            srp: if merged.srp_struct.is_empty()
                && merged.srp_module.is_empty()
                && merged.srp_param.is_empty()
            {
                None
            } else {
                Some(super::json_types::JsonSrp {
                    struct_warnings: merged.srp_struct,
                    module_warnings: merged.srp_module,
                    param_warnings: merged.srp_param,
                })
            },
            orphan_suppressions,
            architecture_findings: merged.architecture,
        };
        serde_json::to_string_pretty(&output)
            .unwrap_or_else(|e| format!("{{\"error\":\"JSON serialization failed: {e}\"}}"))
    }
}

/// Print results in a machine-readable format (for CI integration).
/// Trivial: delegates to build_json_string and prints.
pub fn print_json(analysis: &AnalysisResult) {
    let json = build_json_string(analysis);
    println!("{json}");
}

/// Build a JSON string from analysis results.
/// Trivial: instantiate composer + reporter, render.
pub(crate) fn build_json_string(analysis: &AnalysisResult) -> String {
    let composer = JsonOutputComposer {
        summary: &analysis.summary,
    };
    let reporter = JsonReporter {
        findings: &analysis.findings,
        composer: &composer,
    };
    reporter.render(&analysis.findings, &analysis.data)
}
