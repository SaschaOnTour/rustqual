use super::json_types::{
    JsonBoilerplateFind, JsonComplexity, JsonCoupling, JsonCouplingModule, JsonDeadCodeWarning,
    JsonDuplicateEntry, JsonDuplicateGroup, JsonFragmentEntry, JsonFragmentGroup, JsonFunction,
    JsonHotspot, JsonMagicNumber, JsonOutput, JsonRepeatedMatchEntry, JsonRepeatedMatchGroup,
    JsonSdpViolation, JsonSummary, JsonWildcardWarning,
};
use super::{json_srp, json_structural, json_tq, AnalysisResult};
use crate::analyzer::Classification;

/// Print results in a machine-readable format (for CI integration).
/// Integration: delegates to build_json_string and prints.
pub fn print_json(analysis: &AnalysisResult) {
    let json = build_json_string(analysis);
    println!("{json}");
}

/// Build a JSON string from analysis results.
/// Operation: serialization logic with match on classification in closures.
fn build_json_string(analysis: &AnalysisResult) -> String {
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
                crate::dry::DuplicateKind::Exact => ("exact".to_string(), None),
                crate::dry::DuplicateKind::NearDuplicate { similarity } => {
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
                crate::dry::DeadCodeKind::Uncalled => "uncalled",
                crate::dry::DeadCodeKind::TestOnly => "test_only",
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

    serde_json::to_string_pretty(&output).expect("JSON serialization failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::{
        compute_severity, CallOccurrence, Classification, ComplexityMetrics, FunctionAnalysis,
        LogicOccurrence,
    };
    use crate::report::Summary;

    fn make_result(name: &str, classification: Classification) -> FunctionAnalysis {
        let severity = compute_severity(&classification);
        FunctionAnalysis {
            name: name.to_string(),
            file: "test.rs".to_string(),
            line: 1,
            classification,
            parent_type: None,
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
            is_trait_impl: false,
            is_test: false,
            effort_score: None,
        }
    }

    fn make_analysis(results: Vec<FunctionAnalysis>) -> AnalysisResult {
        let summary = Summary::from_results(&results);
        AnalysisResult {
            results,
            summary,
            coupling: None,
            duplicates: vec![],
            dead_code: vec![],
            fragments: vec![],
            boilerplate: vec![],
            wildcard_warnings: vec![],
            repeated_matches: vec![],
            srp: None,
            tq: None,
            structural: None,
        }
    }

    #[test]
    fn test_print_json_empty_no_panic() {
        let analysis = make_analysis(vec![]);
        print_json(&analysis);
    }

    #[test]
    fn test_print_json_violation_no_panic() {
        let analysis = make_analysis(vec![make_result(
            "bad_fn",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "if".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "f".into(),
                    line: 2,
                }],
            },
        )]);
        print_json(&analysis);
    }

    #[test]
    fn test_print_json_all_types_no_panic() {
        let analysis = make_analysis(vec![
            make_result("a", Classification::Integration),
            make_result("b", Classification::Operation),
            make_result("c", Classification::Trivial),
            make_result(
                "d",
                Classification::Violation {
                    has_logic: true,
                    has_own_calls: true,
                    logic_locations: vec![LogicOccurrence {
                        kind: "match".into(),
                        line: 1,
                    }],
                    call_locations: vec![CallOccurrence {
                        name: "g".into(),
                        line: 2,
                    }],
                },
            ),
        ]);
        print_json(&analysis);
    }

    #[test]
    fn test_print_json_with_complexity_no_panic() {
        let mut func = make_result("f", Classification::Operation);
        func.complexity = Some(ComplexityMetrics {
            logic_count: 3,
            call_count: 0,
            max_nesting: 2,
            ..Default::default()
        });
        let analysis = make_analysis(vec![func]);
        print_json(&analysis);
    }

    #[test]
    fn test_print_json_suppressed_no_panic() {
        let mut func = make_result(
            "suppressed",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![LogicOccurrence {
                    kind: "if".into(),
                    line: 1,
                }],
                call_locations: vec![CallOccurrence {
                    name: "f".into(),
                    line: 2,
                }],
            },
        );
        func.suppressed = true;
        let analysis = make_analysis(vec![func]);
        print_json(&analysis);
    }

    #[test]
    fn test_print_json_high_severity_no_panic() {
        let analysis = make_analysis(vec![make_result(
            "complex",
            Classification::Violation {
                has_logic: true,
                has_own_calls: true,
                logic_locations: vec![
                    LogicOccurrence {
                        kind: "if".into(),
                        line: 1,
                    },
                    LogicOccurrence {
                        kind: "match".into(),
                        line: 2,
                    },
                    LogicOccurrence {
                        kind: "for".into(),
                        line: 3,
                    },
                ],
                call_locations: vec![
                    CallOccurrence {
                        name: "a".into(),
                        line: 4,
                    },
                    CallOccurrence {
                        name: "b".into(),
                        line: 5,
                    },
                    CallOccurrence {
                        name: "c".into(),
                        line: 6,
                    },
                ],
            },
        )]);
        print_json(&analysis);
    }

    // ── JSON content tests (verifying fields are present) ──────

    #[test]
    fn test_json_summary_has_complexity_warnings_field() {
        let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
        let json = build_json_string(&analysis);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            parsed["summary"]["complexity_warnings"].is_number(),
            "JSON summary must include complexity_warnings field"
        );
    }

    #[test]
    fn test_json_summary_has_magic_number_warnings_field() {
        let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
        let json = build_json_string(&analysis);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            parsed["summary"]["magic_number_warnings"].is_number(),
            "JSON summary must include magic_number_warnings field"
        );
    }

    #[test]
    fn test_json_summary_has_all_dimension_fields() {
        let analysis = make_analysis(vec![make_result("f", Classification::Operation)]);
        let json = build_json_string(&analysis);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let s = &parsed["summary"];
        let expected_fields = [
            "total",
            "integrations",
            "operations",
            "violations",
            "trivial",
            "suppressed",
            "all_suppressions",
            "iosp_score",
            "quality_score",
            "complexity_warnings",
            "magic_number_warnings",
            "nesting_depth_warnings",
            "function_length_warnings",
            "unsafe_warnings",
            "error_handling_warnings",
            "coupling_warnings",
            "coupling_cycles",
            "duplicate_groups",
            "dead_code_warnings",
            "fragment_groups",
            "boilerplate_warnings",
            "srp_struct_warnings",
            "srp_module_warnings",
            "srp_param_warnings",
            "tq_no_assertion_warnings",
            "tq_no_sut_warnings",
            "tq_untested_warnings",
            "tq_uncovered_warnings",
            "tq_untested_logic_warnings",
            "suppression_ratio_exceeded",
        ];
        expected_fields.iter().for_each(|&field| {
            assert!(!s[field].is_null(), "JSON summary missing field: {field}");
        });
    }

    #[test]
    fn test_json_complexity_has_extended_fields() {
        let mut func = make_result("f", Classification::Operation);
        func.complexity = Some(ComplexityMetrics {
            logic_count: 3,
            call_count: 1,
            max_nesting: 2,
            function_lines: 45,
            unsafe_blocks: 1,
            unwrap_count: 2,
            expect_count: 1,
            panic_count: 0,
            todo_count: 0,
            ..Default::default()
        });
        let analysis = make_analysis(vec![func]);
        let json = build_json_string(&analysis);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let c = &parsed["functions"][0]["complexity"];
        assert_eq!(c["function_lines"].as_u64().unwrap(), 45);
        assert_eq!(c["unsafe_blocks"].as_u64().unwrap(), 1);
        assert_eq!(c["unwrap_count"].as_u64().unwrap(), 2);
        assert_eq!(c["expect_count"].as_u64().unwrap(), 1);
        assert_eq!(c["panic_count"].as_u64().unwrap(), 0);
        assert_eq!(c["todo_count"].as_u64().unwrap(), 0);
    }
}
