use super::json_types::{
    JsonBoilerplateFind, JsonComplexity, JsonCoupling, JsonCouplingModule, JsonDeadCodeWarning,
    JsonDuplicateEntry, JsonDuplicateGroup, JsonFragmentEntry, JsonFragmentGroup, JsonFunction,
    JsonHotspot, JsonMagicNumber, JsonOutput, JsonRepeatedMatchEntry, JsonRepeatedMatchGroup,
    JsonSdpViolation, JsonSummary, JsonWildcardWarning,
};
use super::{json_srp, json_structural, json_tq, AnalysisResult};
use crate::adapters::analyzers::iosp::Classification;

/// Print results in a machine-readable format (for CI integration).
/// Integration: delegates to build_json_string and prints.
pub fn print_json(analysis: &AnalysisResult) {
    let json = build_json_string(analysis);
    println!("{json}");
}

/// Build a JSON string from analysis results.
/// Operation: serialization logic with match on classification in closures.
pub(crate) fn build_json_string(analysis: &AnalysisResult) -> String {
    let results = &analysis.results;
    let summary = &analysis.summary;
    let coupling = analysis.coupling.as_ref();
    let duplicates = &analysis.duplicates;
    let dead_code = &analysis.dead_code;
    let fragments = &analysis.fragments;
    let boilerplate = &analysis.boilerplate;
    let srp = analysis.srp.as_ref();
    use std::collections::BTreeMap;

    let functions: Vec<JsonFunction> = results
        .iter()
        .map(|f| {
            let (classification, logic, calls) = match &f.classification {
                Classification::Integration => ("integration".to_string(), vec![], vec![]),
                Classification::Operation => ("operation".to_string(), vec![], vec![]),
                Classification::Trivial => ("trivial".to_string(), vec![], vec![]),
                Classification::Violation {
                    logic_locations,
                    call_locations,
                    ..
                } => {
                    let logic = logic_locations
                        .iter()
                        .map(|l| {
                            let mut m = BTreeMap::new();
                            m.insert("kind".into(), l.kind.clone());
                            m.insert("line".into(), l.line.to_string());
                            m
                        })
                        .collect();
                    let calls = call_locations
                        .iter()
                        .map(|c| {
                            let mut m = BTreeMap::new();
                            m.insert("name".into(), c.name.clone());
                            m.insert("line".into(), c.line.to_string());
                            m
                        })
                        .collect();
                    ("violation".to_string(), logic, calls)
                }
            };
            let complexity = f.complexity.as_ref().map(|c| JsonComplexity {
                logic_count: c.logic_count,
                call_count: c.call_count,
                max_nesting: c.max_nesting,
                cognitive_complexity: c.cognitive_complexity,
                cyclomatic_complexity: c.cyclomatic_complexity,
                function_lines: c.function_lines,
                unsafe_blocks: c.unsafe_blocks,
                unwrap_count: c.unwrap_count,
                expect_count: c.expect_count,
                panic_count: c.panic_count,
                todo_count: c.todo_count,
                hotspots: c
                    .hotspots
                    .iter()
                    .map(|h| JsonHotspot {
                        line: h.line,
                        nesting_depth: h.nesting_depth,
                        construct: h.construct.clone(),
                    })
                    .collect(),
                magic_numbers: c
                    .magic_numbers
                    .iter()
                    .map(|m| JsonMagicNumber {
                        line: m.line,
                        value: m.value.clone(),
                    })
                    .collect(),
            });
            JsonFunction {
                name: f.name.clone(),
                file: f.file.clone(),
                line: f.line,
                parent_type: f.parent_type.clone(),
                classification,
                severity: f.severity.clone(),
                suppressed: if f.suppressed { Some(true) } else { None },
                logic,
                calls,
                complexity,
                parameter_count: f.parameter_count,
                is_trait_impl: f.is_trait_impl,
                effort_score: f.effort_score,
            }
        })
        .collect();

    let json_coupling = coupling.map(|ca| JsonCoupling {
        modules: ca
            .metrics
            .iter()
            .map(|m| JsonCouplingModule {
                name: m.module_name.clone(),
                afferent: m.afferent,
                efferent: m.efferent,
                instability: m.instability,
            })
            .collect(),
        cycles: ca.cycles.iter().map(|c| c.modules.clone()).collect(),
        sdp_violations: ca
            .sdp_violations
            .iter()
            .filter(|v| !v.suppressed)
            .map(|v| JsonSdpViolation {
                from_module: v.from_module.clone(),
                to_module: v.to_module.clone(),
                from_instability: v.from_instability,
                to_instability: v.to_instability,
            })
            .collect(),
    });

    let json_duplicates: Vec<JsonDuplicateGroup> = duplicates
        .iter()
        .filter(|g| !g.suppressed)
        .map(|g| {
            let (kind, similarity) = match &g.kind {
                crate::adapters::analyzers::dry::DuplicateKind::Exact => {
                    ("exact".to_string(), None)
                }
                crate::adapters::analyzers::dry::DuplicateKind::NearDuplicate { similarity } => {
                    ("near_duplicate".to_string(), Some(*similarity))
                }
            };
            JsonDuplicateGroup {
                kind,
                similarity,
                entries: g
                    .entries
                    .iter()
                    .map(|e| JsonDuplicateEntry {
                        name: e.name.clone(),
                        qualified_name: e.qualified_name.clone(),
                        file: e.file.clone(),
                        line: e.line,
                    })
                    .collect(),
            }
        })
        .collect();

    let json_dead_code: Vec<JsonDeadCodeWarning> = dead_code
        .iter()
        .map(|w| {
            let kind = match &w.kind {
                crate::adapters::analyzers::dry::DeadCodeKind::Uncalled => "uncalled",
                crate::adapters::analyzers::dry::DeadCodeKind::TestOnly => "test_only",
            };
            JsonDeadCodeWarning {
                function_name: w.function_name.clone(),
                qualified_name: w.qualified_name.clone(),
                file: w.file.clone(),
                line: w.line,
                kind: kind.to_string(),
                suggestion: w.suggestion.clone(),
            }
        })
        .collect();

    let json_fragments: Vec<JsonFragmentGroup> = fragments
        .iter()
        .filter(|g| !g.suppressed)
        .map(|g| JsonFragmentGroup {
            statement_count: g.statement_count,
            entries: g
                .entries
                .iter()
                .map(|e| JsonFragmentEntry {
                    function_name: e.function_name.clone(),
                    qualified_name: e.qualified_name.clone(),
                    file: e.file.clone(),
                    start_line: e.start_line,
                    end_line: e.end_line,
                })
                .collect(),
        })
        .collect();

    let json_tq_warnings = json_tq::build_tq_warnings(analysis);
    let json_structural_warnings = json_structural::build_structural_warnings(analysis);

    let output = JsonOutput {
        summary: JsonSummary {
            total: summary.total,
            integrations: summary.integrations,
            operations: summary.operations,
            violations: summary.violations,
            trivial: summary.trivial,
            suppressed: summary.suppressed,
            all_suppressions: summary.all_suppressions,
            iosp_score: summary.iosp_score,
            quality_score: summary.quality_score,
            complexity_warnings: summary.complexity_warnings,
            magic_number_warnings: summary.magic_number_warnings,
            coupling_warnings: summary.coupling_warnings,
            coupling_cycles: summary.coupling_cycles,
            duplicate_groups: summary.duplicate_groups,
            dead_code_warnings: summary.dead_code_warnings,
            fragment_groups: summary.fragment_groups,
            boilerplate_warnings: summary.boilerplate_warnings,
            srp_struct_warnings: summary.srp_struct_warnings,
            srp_module_warnings: summary.srp_module_warnings,
            srp_param_warnings: summary.srp_param_warnings,
            nesting_depth_warnings: summary.nesting_depth_warnings,
            function_length_warnings: summary.function_length_warnings,
            unsafe_warnings: summary.unsafe_warnings,
            error_handling_warnings: summary.error_handling_warnings,
            wildcard_import_warnings: summary.wildcard_import_warnings,
            sdp_violations: summary.sdp_violations,
            tq_no_assertion_warnings: summary.tq_no_assertion_warnings,
            tq_no_sut_warnings: summary.tq_no_sut_warnings,
            tq_untested_warnings: summary.tq_untested_warnings,
            tq_uncovered_warnings: summary.tq_uncovered_warnings,
            tq_untested_logic_warnings: summary.tq_untested_logic_warnings,
            structural_srp_warnings: summary.structural_srp_warnings,
            structural_coupling_warnings: summary.structural_coupling_warnings,
            repeated_match_groups: summary.repeated_match_groups,
            suppression_ratio_exceeded: summary.suppression_ratio_exceeded,
        },
        functions,
        coupling: json_coupling,
        duplicates: json_duplicates,
        dead_code: json_dead_code,
        fragments: json_fragments,
        wildcard_warnings: analysis
            .wildcard_warnings
            .iter()
            .filter(|w| !w.suppressed)
            .map(|w| JsonWildcardWarning {
                file: w.file.clone(),
                line: w.line,
                module_path: w.module_path.clone(),
            })
            .collect(),
        boilerplate: boilerplate
            .iter()
            .map(|b| JsonBoilerplateFind {
                pattern_id: b.pattern_id.clone(),
                file: b.file.clone(),
                line: b.line,
                struct_name: b.struct_name.clone(),
                description: b.description.clone(),
                suggestion: b.suggestion.clone(),
            })
            .collect(),
        tq_warnings: json_tq_warnings,
        structural_warnings: json_structural_warnings,
        repeated_matches: analysis
            .repeated_matches
            .iter()
            .map(|g| JsonRepeatedMatchGroup {
                enum_name: g.enum_name.clone(),
                entries: g
                    .entries
                    .iter()
                    .map(|e| JsonRepeatedMatchEntry {
                        file: e.file.clone(),
                        line: e.line,
                        function_name: e.function_name.clone(),
                        arm_count: e.arm_count,
                    })
                    .collect(),
            })
            .collect(),
        srp: srp.map(json_srp::build_json_srp),
    };

    serde_json::to_string_pretty(&output)
        .unwrap_or_else(|e| format!("{{\"error\":\"JSON serialization failed: {e}\"}}"))
}
