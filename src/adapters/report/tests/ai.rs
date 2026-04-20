use crate::report::ai::*;
use crate::report::AnalysisResult;

// ── Category mapping tests ──────────────────────────────────

#[test]
fn test_map_category_all_known() {
    let cases = [
        ("VIOLATION", "violation"),
        ("COGNITIVE", "cognitive_complexity"),
        ("CYCLOMATIC", "cyclomatic_complexity"),
        ("MAGIC_NUMBER", "magic_number"),
        ("NESTING", "nesting_depth"),
        ("LONG_FN", "long_function"),
        ("UNSAFE", "unsafe_block"),
        ("ERROR_HANDLING", "error_handling"),
        ("DUPLICATE", "duplicate"),
        ("DEAD_CODE", "dead_code"),
        ("FRAGMENT", "fragment"),
        ("BOILERPLATE", "boilerplate"),
        ("WILDCARD", "wildcard_import"),
        ("REPEATED_MATCH", "repeated_match"),
        ("SRP_STRUCT", "srp_struct"),
        ("SRP_MODULE", "srp_module"),
        ("SRP_PARAMS", "srp_params"),
        ("COUPLING", "coupling"),
        ("CYCLE", "cycle"),
        ("SDP", "sdp_violation"),
        ("TQ_NO_ASSERT", "no_assertion"),
        ("TQ_NO_SUT", "no_sut_call"),
        ("TQ_UNTESTED", "untested"),
        ("TQ_UNCOVERED", "uncovered"),
        ("TQ_UNTESTED_LOGIC", "untested_logic"),
        ("STRUCTURAL", "structural"),
        ("ORPHAN_SUPPRESSION", "orphan_suppression"),
    ];
    cases.iter().for_each(|(input, expected)| {
        assert_eq!(
            map_category(input),
            *expected,
            "map_category({input}) should return {expected}"
        );
    });
}

fn empty_analysis() -> AnalysisResult {
    AnalysisResult {
        results: vec![],
        summary: crate::report::Summary::default(),
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
        architecture_findings: vec![],
        orphan_suppressions: vec![],
    }
}

#[test]
fn test_build_ai_value_zero_findings() {
    let analysis = empty_analysis();
    let config = crate::config::Config::default();
    let value = build_ai_value(&analysis, &config);

    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["findings"], 0);
    assert!(
        value.get("findings_by_file").is_none(),
        "no findings_by_file when 0 findings"
    );
}

#[test]
fn test_build_findings_grouped_by_file() {
    use crate::report::findings_list::FindingEntry;

    let entries = vec![
        FindingEntry {
            file: "src/a.rs".into(),
            line: 10,
            category: "MAGIC_NUMBER",
            detail: "42".into(),
            function_name: "fn_a".into(),
        },
        FindingEntry {
            file: "src/a.rs".into(),
            line: 20,
            category: "LONG_FN",
            detail: "72 lines".into(),
            function_name: "fn_b".into(),
        },
        FindingEntry {
            file: "src/b.rs".into(),
            line: 5,
            category: "DUPLICATE",
            detail: "exact".into(),
            function_name: "fn_c".into(),
        },
    ];

    let analysis = empty_analysis();
    let config = crate::config::Config::default();
    let value = build_findings_value(&entries, &analysis, &config);
    let obj = value.as_object().unwrap();

    // Two file groups
    assert_eq!(obj.len(), 2, "should group into 2 files");
    assert!(obj.contains_key("src/a.rs"));
    assert!(obj.contains_key("src/b.rs"));

    // src/a.rs has 2 entries
    let a_entries = obj["src/a.rs"].as_array().unwrap();
    assert_eq!(a_entries.len(), 2);
    assert_eq!(a_entries[0]["category"], "magic_number");
    assert_eq!(a_entries[0]["line"], 10);
    assert_eq!(a_entries[0]["fn"], "fn_a");
    assert_eq!(a_entries[0]["detail"], "42");
    assert_eq!(a_entries[1]["category"], "long_function");
    assert_eq!(a_entries[1]["line"], 20);

    // src/b.rs has 1 entry
    let b_entries = obj["src/b.rs"].as_array().unwrap();
    assert_eq!(b_entries.len(), 1);
    assert_eq!(b_entries[0]["category"], "duplicate");
    assert_eq!(b_entries[0]["fn"], "fn_c");
}

#[test]
fn test_enrich_violation_detail() {
    use crate::adapters::analyzers::iosp::{
        CallOccurrence, Classification, FunctionAnalysis, LogicOccurrence,
    };
    use crate::report::findings_list::FindingEntry;

    let mut analysis = empty_analysis();
    analysis.results = vec![FunctionAnalysis {
        name: "bad_fn".into(),
        file: "src/lib.rs".into(),
        line: 40,
        classification: Classification::Violation {
            has_logic: true,
            has_own_calls: true,
            logic_locations: vec![
                LogicOccurrence {
                    line: 44,
                    kind: "if".into(),
                },
                LogicOccurrence {
                    line: 47,
                    kind: "for".into(),
                },
            ],
            call_locations: vec![CallOccurrence {
                line: 50,
                name: "helper".into(),
            }],
        },
        parent_type: None,
        suppressed: false,
        complexity: None,
        qualified_name: "bad_fn".into(),
        severity: None,
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
    }];

    let entries = vec![FindingEntry {
        file: "src/lib.rs".into(),
        line: 40,
        category: "VIOLATION",
        detail: "logic + calls".into(),
        function_name: "bad_fn".into(),
    }];

    let config = crate::config::Config::default();
    let value = build_findings_value(&entries, &analysis, &config);
    let arr = value["src/lib.rs"].as_array().unwrap();
    let detail = arr[0]["detail"].as_str().unwrap();
    assert!(
        detail.contains("logic lines 44,47"),
        "detail should show logic lines, got: {detail}"
    );
    assert!(
        detail.contains("call lines 50"),
        "detail should show call lines, got: {detail}"
    );
}

#[test]
fn test_enrich_duplicate_detail() {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };
    use crate::report::findings_list::FindingEntry;

    let mut analysis = empty_analysis();
    analysis.duplicates = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "fn_a".into(),
                qualified_name: "mod::fn_a".into(),
                file: "src/a.rs".into(),
                line: 10,
            },
            DuplicateEntry {
                name: "fn_b".into(),
                qualified_name: "mod::fn_b".into(),
                file: "src/b.rs".into(),
                line: 20,
            },
        ],
        kind: DuplicateKind::Exact,
        suppressed: false,
    }];

    let entries = vec![
        FindingEntry {
            file: "src/a.rs".into(),
            line: 10,
            category: "DUPLICATE",
            detail: "exact".into(),
            function_name: "mod::fn_a".into(),
        },
        FindingEntry {
            file: "src/b.rs".into(),
            line: 20,
            category: "DUPLICATE",
            detail: "exact".into(),
            function_name: "mod::fn_b".into(),
        },
    ];

    let config = crate::config::Config::default();
    let value = build_findings_value(&entries, &analysis, &config);
    let a_detail = value["src/a.rs"].as_array().unwrap()[0]["detail"]
        .as_str()
        .unwrap();
    let b_detail = value["src/b.rs"].as_array().unwrap()[0]["detail"]
        .as_str()
        .unwrap();
    assert!(
        a_detail.contains("src/b.rs:20"),
        "should reference partner location, got: {a_detail}"
    );
    assert!(
        b_detail.contains("src/a.rs:10"),
        "should reference partner location, got: {b_detail}"
    );
}

#[test]
fn test_enrich_fragment_detail() {
    use crate::adapters::analyzers::dry::fragments::{FragmentEntry, FragmentGroup};
    use crate::report::findings_list::FindingEntry;

    let mut analysis = empty_analysis();
    analysis.fragments = vec![FragmentGroup {
        entries: vec![
            FragmentEntry {
                function_name: "fn_a".into(),
                qualified_name: "fn_a".into(),
                file: "src/a.rs".into(),
                start_line: 10,
                end_line: 15,
            },
            FragmentEntry {
                function_name: "fn_b".into(),
                qualified_name: "fn_b".into(),
                file: "src/b.rs".into(),
                start_line: 30,
                end_line: 35,
            },
        ],
        statement_count: 3,
        suppressed: false,
    }];

    let entries = vec![
        FindingEntry {
            file: "src/a.rs".into(),
            line: 10,
            category: "FRAGMENT",
            detail: "3 stmts".into(),
            function_name: "fn_a".into(),
        },
        FindingEntry {
            file: "src/b.rs".into(),
            line: 30,
            category: "FRAGMENT",
            detail: "3 stmts".into(),
            function_name: "fn_b".into(),
        },
    ];

    let config = crate::config::Config::default();
    let value = build_findings_value(&entries, &analysis, &config);
    let a_detail = value["src/a.rs"].as_array().unwrap()[0]["detail"]
        .as_str()
        .unwrap();
    assert!(
        a_detail.contains("also in src/b.rs:30"),
        "should reference partner, got: {a_detail}"
    );
}

#[test]
fn test_global_findings_not_dropped() {
    use crate::report::findings_list::FindingEntry;
    let entries = vec![
        FindingEntry {
            file: "".into(),
            line: 0,
            category: "COUPLING",
            detail: "I=0.71 Ca=2 Ce=5".into(),
            function_name: "db".into(),
        },
        FindingEntry {
            file: "src/a.rs".into(),
            line: 10,
            category: "MAGIC_NUMBER",
            detail: "42".into(),
            function_name: "fn_a".into(),
        },
    ];
    let analysis = empty_analysis();
    let config = crate::config::Config::default();
    let value = build_findings_value(&entries, &analysis, &config);
    let obj = value.as_object().unwrap();
    assert!(
        obj.contains_key(GLOBAL_FILE_KEY),
        "empty-file findings should be under GLOBAL_FILE_KEY"
    );
    assert!(obj.contains_key("src/a.rs"));
    let global = obj[GLOBAL_FILE_KEY].as_array().unwrap();
    assert_eq!(global.len(), 1);
    assert_eq!(global[0]["category"], "coupling");
}

#[test]
fn test_enrich_complexity_detail() {
    use crate::report::findings_list::FindingEntry;
    let analysis = empty_analysis();
    let entry = FindingEntry {
        file: "src/lib.rs".into(),
        line: 10,
        category: "COGNITIVE",
        detail: "complexity 12".into(),
        function_name: "fn1".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(detail.contains("12"), "should contain value, got: {detail}");
    assert!(
        detail.contains(&format!("max {}", config.complexity.max_cognitive)),
        "should contain threshold, got: {detail}"
    );
}

#[test]
fn test_enrich_long_function_detail() {
    use crate::report::findings_list::FindingEntry;
    let analysis = empty_analysis();
    let entry = FindingEntry {
        file: "src/lib.rs".into(),
        line: 10,
        category: "LONG_FN",
        detail: "72 lines".into(),
        function_name: "fn1".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("72 lines"),
        "should contain line count, got: {detail}"
    );
    assert!(
        detail.contains(&format!("max {}", config.complexity.max_function_lines)),
        "should contain threshold, got: {detail}"
    );
}

#[test]
fn test_enrich_nesting_detail() {
    use crate::report::findings_list::FindingEntry;
    let analysis = empty_analysis();
    let entry = FindingEntry {
        file: "src/lib.rs".into(),
        line: 10,
        category: "NESTING",
        detail: "depth 5".into(),
        function_name: "fn1".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("depth 5"),
        "should contain depth, got: {detail}"
    );
    assert!(
        detail.contains(&format!("max {}", config.complexity.max_nesting_depth)),
        "should contain threshold, got: {detail}"
    );
}

#[test]
fn test_enrich_srp_struct_detail() {
    use crate::adapters::analyzers::srp::{SrpAnalysis, SrpWarning};
    use crate::report::findings_list::FindingEntry;
    let mut analysis = empty_analysis();
    analysis.srp = Some(SrpAnalysis {
        struct_warnings: vec![SrpWarning {
            struct_name: "BigStruct".into(),
            file: "src/lib.rs".into(),
            line: 10,
            lcom4: 3,
            field_count: 8,
            method_count: 12,
            fan_out: 5,
            composite_score: 0.85,
            clusters: vec![],
            suppressed: false,
        }],
        module_warnings: vec![],
        param_warnings: vec![],
    });
    let entry = FindingEntry {
        file: "src/lib.rs".into(),
        line: 10,
        category: "SRP_STRUCT",
        detail: "LCOM4=3".into(),
        function_name: "BigStruct".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("LCOM4=3"),
        "should contain LCOM4, got: {detail}"
    );
    assert!(
        detail.contains("12 methods"),
        "should contain method count, got: {detail}"
    );
    assert!(
        detail.contains("8 fields"),
        "should contain field count, got: {detail}"
    );
}

#[test]
fn test_enrich_srp_module_detail() {
    // Length-only driver: keeps historical "N lines (max M)" shape.
    use crate::adapters::analyzers::srp::{ModuleSrpWarning, SrpAnalysis};
    use crate::report::findings_list::FindingEntry;
    let mut analysis = empty_analysis();
    analysis.srp = Some(SrpAnalysis {
        struct_warnings: vec![],
        module_warnings: vec![ModuleSrpWarning {
            module: "src/lib.rs".into(),
            file: "src/lib.rs".into(),
            production_lines: 310,
            length_score: 0.05,
            independent_clusters: 1,
            cluster_names: vec![],
            suppressed: false,
        }],
        param_warnings: vec![],
    });
    let entry = FindingEntry {
        file: "src/lib.rs".into(),
        line: 1,
        category: "SRP_MODULE",
        detail: "310 lines".into(),
        function_name: "lib".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("310 lines"),
        "should contain line count, got: {detail}"
    );
    assert!(
        detail.contains(&format!("max {}", config.srp.file_length_baseline)),
        "should contain threshold, got: {detail}"
    );
}

#[test]
fn ai_srp_module_cluster_driver_is_named() {
    // Bug 4 primary fix: when independent_clusters drives the finding
    // (and length_score == 0), AI detail must surface that instead of
    // the misleading "N lines (max M)".
    use crate::adapters::analyzers::srp::{ModuleSrpWarning, SrpAnalysis};
    use crate::report::findings_list::FindingEntry;
    let mut analysis = empty_analysis();
    analysis.srp = Some(SrpAnalysis {
        struct_warnings: vec![],
        module_warnings: vec![ModuleSrpWarning {
            module: "tests/pq_tests.rs".into(),
            file: "tests/pq_tests.rs".into(),
            production_lines: 96,
            length_score: 0.0,
            independent_clusters: 4,
            cluster_names: vec![
                vec!["test_a".into()],
                vec!["test_b".into()],
                vec!["test_c".into()],
                vec!["test_d".into()],
            ],
            suppressed: false,
        }],
        param_warnings: vec![],
    });
    let entry = FindingEntry {
        file: "tests/pq_tests.rs".into(),
        line: 1,
        category: "SRP_MODULE",
        detail: "96 lines".into(),
        function_name: "pq_tests".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("4 independent clusters"),
        "cluster-driven module should report clusters, got: {detail}"
    );
    assert!(
        detail.contains(&format!("max {}", config.srp.max_independent_clusters)),
        "cluster detail should include max_independent_clusters, got: {detail}"
    );
}

#[test]
fn ai_srp_module_combined_when_both_drivers_active() {
    // Both length_score > 0 AND cluster count exceeds → both drivers
    // must be in the detail so the AI agent sees the full picture.
    use crate::adapters::analyzers::srp::{ModuleSrpWarning, SrpAnalysis};
    use crate::report::findings_list::FindingEntry;
    let mut analysis = empty_analysis();
    analysis.srp = Some(SrpAnalysis {
        struct_warnings: vec![],
        module_warnings: vec![ModuleSrpWarning {
            module: "src/big.rs".into(),
            file: "src/big.rs".into(),
            production_lines: 850,
            length_score: 1.0,
            independent_clusters: 4,
            cluster_names: vec![vec!["a".into()], vec!["b".into()]],
            suppressed: false,
        }],
        param_warnings: vec![],
    });
    let entry = FindingEntry {
        file: "src/big.rs".into(),
        line: 1,
        category: "SRP_MODULE",
        detail: "850 lines".into(),
        function_name: "big".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("850 lines"),
        "combined detail should retain line count, got: {detail}"
    );
    assert!(
        detail.contains("4 independent clusters"),
        "combined detail should include cluster count, got: {detail}"
    );
}

#[test]
fn ai_sdp_includes_instabilities() {
    // SDP violations already carry from/to instability values —
    // surface them in AI so the agent can see the concrete gap.
    use crate::adapters::analyzers::coupling::sdp::SdpViolation;
    use crate::adapters::analyzers::coupling::{CouplingAnalysis, ModuleGraph};
    use crate::report::findings_list::FindingEntry;
    let mut analysis = empty_analysis();
    analysis.coupling = Some(CouplingAnalysis {
        metrics: vec![],
        cycles: vec![],
        sdp_violations: vec![SdpViolation {
            from_module: "a".into(),
            to_module: "b".into(),
            from_instability: 0.1,
            to_instability: 0.9,
            suppressed: false,
        }],
        graph: ModuleGraph::default(),
    });
    let entry = FindingEntry {
        file: "".into(),
        line: 0,
        category: "SDP",
        detail: "a -> b".into(),
        function_name: "a".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("0.1") && detail.contains("0.9"),
        "SDP detail should name both instabilities, got: {detail}"
    );
}

#[test]
fn ai_boilerplate_includes_suggestion() {
    // BoilerplateFind already carries description + suggestion — the
    // AI agent needs both to act without a JSON round-trip.
    use crate::adapters::analyzers::dry::boilerplate::BoilerplateFind;
    use crate::report::findings_list::FindingEntry;
    let mut analysis = empty_analysis();
    analysis.boilerplate = vec![BoilerplateFind {
        pattern_id: "BP-005".into(),
        file: "src/foo.rs".into(),
        line: 42,
        struct_name: Some("Foo".into()),
        description: "manual Default impl".into(),
        suggestion: "use #[derive(Default)]".into(),
        suppressed: false,
    }];
    let entry = FindingEntry {
        file: "src/foo.rs".into(),
        line: 42,
        category: "BOILERPLATE",
        detail: "BP-005".into(),
        function_name: "Foo".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("#[derive(Default)]"),
        "boilerplate detail should include the suggestion, got: {detail}"
    );
}

#[test]
fn ai_dead_code_includes_suggestion() {
    // DeadCodeWarning.suggestion carries the actionable hint (delete /
    // move / annotate). Surface it in AI so a one-shot read is enough.
    use crate::adapters::analyzers::dry::dead_code::{DeadCodeKind, DeadCodeWarning};
    use crate::report::findings_list::FindingEntry;
    let mut analysis = empty_analysis();
    analysis.dead_code = vec![DeadCodeWarning {
        function_name: "helper".into(),
        qualified_name: "helper".into(),
        file: "src/a.rs".into(),
        line: 10,
        kind: DeadCodeKind::TestOnly,
        suggestion: "only called from test code; move or annotate".into(),
    }];
    let entry = FindingEntry {
        file: "src/a.rs".into(),
        line: 10,
        category: "DEAD_CODE",
        detail: "testonly".into(),
        function_name: "helper".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("move or annotate"),
        "dead_code detail should include suggestion, got: {detail}"
    );
}

#[test]
fn ai_orphan_suppression_detail_includes_reason() {
    // Orphan suppression with a reason: AI detail must carry the
    // original rationale so the agent can judge whether the marker
    // was stale or misplaced.
    use crate::adapters::report::OrphanSuppressionWarning;
    use crate::report::findings_list::FindingEntry;
    let mut analysis = empty_analysis();
    analysis.orphan_suppressions = vec![OrphanSuppressionWarning {
        file: "src/foo.rs".into(),
        line: 12,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: Some("legacy helper, keep for one release".into()),
    }];
    let entry = FindingEntry {
        file: "src/foo.rs".into(),
        line: 12,
        category: "ORPHAN_SUPPRESSION",
        detail: "stale qual:allow(srp)".into(),
        function_name: String::new(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("legacy helper"),
        "orphan detail should carry the original reason, got: {detail}"
    );
}

#[test]
fn ai_orphan_without_reason_has_no_em_dash() {
    use crate::adapters::report::OrphanSuppressionWarning;
    use crate::report::findings_list::FindingEntry;
    let mut analysis = empty_analysis();
    analysis.orphan_suppressions = vec![OrphanSuppressionWarning {
        file: "src/foo.rs".into(),
        line: 12,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: None,
    }];
    let entry = FindingEntry {
        file: "src/foo.rs".into(),
        line: 12,
        category: "ORPHAN_SUPPRESSION",
        detail: "stale qual:allow(srp)".into(),
        function_name: String::new(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert_eq!(
        detail, "stale qual:allow(srp)",
        "no reason → detail stays bare, got: {detail}"
    );
}

#[test]
fn ai_build_ai_value_surfaces_orphan_suppressions() {
    // End-to-end: an AnalysisResult with only an orphan (no other
    // findings) must produce a one-call AI output that contains the
    // orphan's file / line / category.
    use crate::adapters::report::OrphanSuppressionWarning;
    let mut analysis = empty_analysis();
    analysis.orphan_suppressions = vec![OrphanSuppressionWarning {
        file: "src/foo.rs".into(),
        line: 42,
        dimensions: vec![crate::findings::Dimension::Srp],
        reason: None,
    }];
    let config = crate::config::Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(value["findings"], 1);
    let by_file = value
        .get("findings_by_file")
        .expect("findings_by_file present");
    let arr = by_file["src/foo.rs"].as_array().expect("file group");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["category"], "orphan_suppression");
    assert_eq!(arr[0]["line"], 42);
    assert!(
        arr[0]["detail"]
            .as_str()
            .unwrap()
            .contains("stale qual:allow"),
        "detail should describe the stale marker, got: {}",
        arr[0]["detail"]
    );
}

#[test]
fn ai_structural_replaces_raw_code_with_detail() {
    // Structural findings ship with both a short code (e.g. "SLM") and
    // a rich detail message ("method does not use self"). AI detail
    // must include the rich message, not just the code.
    use crate::adapters::analyzers::structural::{
        StructuralAnalysis, StructuralWarning, StructuralWarningKind,
    };
    use crate::report::findings_list::FindingEntry;
    let mut analysis = empty_analysis();
    analysis.structural = Some(StructuralAnalysis {
        warnings: vec![StructuralWarning {
            file: "src/x.rs".into(),
            line: 5,
            name: "Foo::bar".into(),
            kind: StructuralWarningKind::SelflessMethod,
            dimension: crate::findings::Dimension::Srp,
            suppressed: false,
        }],
    });
    let entry = FindingEntry {
        file: "src/x.rs".into(),
        line: 5,
        category: "STRUCTURAL",
        detail: "SLM".into(),
        function_name: "Foo::bar".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    // The kind's .detail() should appear in the enriched string so the
    // AI sees the human-readable message, not just the rule code.
    assert!(
        detail.contains("self never referenced"),
        "structural detail should carry rich message, got: {detail}"
    );
}

#[test]
fn test_enrich_srp_params_detail() {
    use crate::report::findings_list::FindingEntry;
    let analysis = empty_analysis();
    let entry = FindingEntry {
        file: "src/lib.rs".into(),
        line: 10,
        category: "SRP_PARAMS",
        detail: "7 params".into(),
        function_name: "fn1".into(),
    };
    let config = crate::config::Config::default();
    let index = build_enrich_index(&analysis);
    let detail = enrich_detail(&entry, &index, &config);
    assert!(
        detail.contains("7 params"),
        "should contain param count, got: {detail}"
    );
    assert!(
        detail.contains(&format!("max {}", config.srp.max_parameters)),
        "should contain threshold, got: {detail}"
    );
}

#[test]
fn test_build_findings_empty() {
    let analysis = empty_analysis();
    let config = crate::config::Config::default();
    let value = build_findings_value(&[], &analysis, &config);
    assert!(value.as_object().unwrap().is_empty());
}

#[test]
fn test_build_ai_value_with_findings() {
    use crate::adapters::analyzers::iosp::{
        Classification, ComplexityMetrics, FunctionAnalysis, MagicNumberOccurrence,
    };

    let mut analysis = empty_analysis();
    let fa = FunctionAnalysis {
        name: "fn1".into(),
        file: "src/lib.rs".into(),
        line: 10,
        classification: Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: Some(ComplexityMetrics {
            magic_numbers: vec![MagicNumberOccurrence {
                line: 12,
                value: "42".into(),
            }],
            ..Default::default()
        }),
        qualified_name: "fn1".into(),
        severity: None,
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
    };
    analysis.results = vec![fa];

    let config = crate::config::Config::default();
    let value = build_ai_value(&analysis, &config);
    assert_eq!(value["findings"], 1);
    assert!(
        value.get("findings_by_file").is_some(),
        "should have findings_by_file"
    );

    let by_file = value["findings_by_file"].as_object().unwrap();
    assert!(by_file.contains_key("src/lib.rs"));
    let entries = by_file["src/lib.rs"].as_array().unwrap();
    assert_eq!(entries[0]["category"], "magic_number");
    assert_eq!(entries[0]["line"], 12);
}

#[test]
fn test_toon_output_contains_version_and_findings() {
    let analysis = empty_analysis();
    let config = crate::config::Config::default();
    let value = build_ai_value(&analysis, &config);
    let toon = toon_encode::encode_toon(&value, 0);
    assert!(toon.contains("version:"), "TOON should contain version key");
    assert!(toon.contains("findings: 0"), "TOON should show 0 findings");
    assert!(
        !toon.contains("findings_by_file"),
        "TOON should not have findings_by_file when 0"
    );
}

#[test]
fn test_ai_json_output_parseable() {
    let analysis = empty_analysis();
    let config = crate::config::Config::default();
    let value = build_ai_value(&analysis, &config);
    let json_str = serde_json::to_string_pretty(&value).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    assert_eq!(parsed["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(parsed["findings"], 0);
}

#[test]
fn test_toon_output_with_findings_has_tabular_format() {
    use crate::adapters::analyzers::iosp::{
        Classification, ComplexityMetrics, FunctionAnalysis, MagicNumberOccurrence,
    };

    let mut analysis = empty_analysis();
    analysis.results = vec![FunctionAnalysis {
        name: "fn1".into(),
        file: "src/lib.rs".into(),
        line: 10,
        classification: Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: Some(ComplexityMetrics {
            magic_numbers: vec![
                MagicNumberOccurrence {
                    line: 12,
                    value: "42".into(),
                },
                MagicNumberOccurrence {
                    line: 15,
                    value: "99".into(),
                },
            ],
            ..Default::default()
        }),
        qualified_name: "fn1".into(),
        severity: None,
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
    }];

    let config = crate::config::Config::default();
    let value = build_ai_value(&analysis, &config);
    let toon = toon_encode::encode_toon(&value, 0);
    assert!(toon.contains("findings: 2"), "should show 2 findings");
    assert!(
        toon.contains("findings_by_file:"),
        "should have findings_by_file section"
    );
    // TOON tabular format: file name as key with [N]{fields}: header
    assert!(toon.contains("src/lib.rs"), "should contain file name");
    assert!(
        toon.contains("magic_number"),
        "should contain mapped category"
    );
}

#[test]
fn test_map_category_unknown_passthrough() {
    assert_eq!(map_category("UNKNOWN_CAT"), "UNKNOWN_CAT");
    assert_eq!(map_category("NEW_FINDING"), "NEW_FINDING");
}
