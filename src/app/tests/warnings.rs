use crate::adapters::analyzers::iosp::{Classification, ComplexityMetrics, FunctionAnalysis};
use crate::app::warnings::*;
use crate::config::Config;
use crate::report::Summary;
use std::collections::{HashMap, HashSet};

// ── count_rust_allow_attrs ────────────────────────────────────

#[test]
fn test_allow_before_cfg_test_excluded() {
    // #[allow(...)] directly before #[cfg(test)] belongs to the test module
    let source = "#[allow(dead_code)]\n#[cfg(test)]\nmod tests {}";
    assert_eq!(count_rust_allow_attrs(source), 0);
}

#[test]
fn test_allow_with_gap_before_cfg_test_counted() {
    // #[allow(...)] with a non-attribute line gap → production code
    let source = "#[allow(dead_code)]\nfn foo() {}\n#[cfg(test)]\nmod tests {}";
    assert_eq!(count_rust_allow_attrs(source), 1);
}

#[test]
fn test_derive_and_allow_before_cfg_test_excluded() {
    // #[derive(Debug)] + #[allow(...)] both part of test module attribute group
    let source = "#[derive(Debug)]\n#[allow(dead_code)]\n#[cfg(test)]\nmod tests {}";
    assert_eq!(count_rust_allow_attrs(source), 0);
}

#[test]
fn test_no_cfg_test_counts_all() {
    let source = "#[allow(dead_code)]\nfn foo() {}\n#[allow(unused)]\nfn bar() {}";
    assert_eq!(count_rust_allow_attrs(source), 2);
}

#[test]
fn test_cfg_test_on_first_line() {
    let source = "#[cfg(test)]\nmod tests {\n#[allow(dead_code)]\nfn x() {}\n}";
    assert_eq!(count_rust_allow_attrs(source), 0);
}

#[test]
fn test_empty_source() {
    assert_eq!(count_rust_allow_attrs(""), 0);
}

#[test]
fn test_production_allow_before_test_section() {
    let source = "#[allow(clippy::too_many_arguments)]\nfn big() {}\n\n#[cfg(test)]\nmod tests {}";
    assert_eq!(count_rust_allow_attrs(source), 1);
}

#[test]
fn test_allow_inside_test_module_excluded() {
    let source = "fn good() {}\n#[cfg(test)]\nmod tests {\n#[allow(dead_code)]\nfn helper() {}\n}";
    assert_eq!(count_rust_allow_attrs(source), 0);
}

// ── apply_extended_warnings ───────────────────────────────────

use crate::adapters::analyzers::iosp::compute_severity;

fn make_func_with_metrics(metrics: ComplexityMetrics) -> FunctionAnalysis {
    let severity = compute_severity(&Classification::Operation);
    FunctionAnalysis {
        name: "test_fn".to_string(),
        file: "test.rs".to_string(),
        line: 1,
        classification: Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: Some(metrics),
        qualified_name: "test_fn".to_string(),
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
        effort_score: None,
        is_test: false,
    }
}

#[test]
fn test_nesting_depth_warning_set() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        max_nesting: 5,
        ..Default::default()
    })];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(results[0].nesting_depth_warning, "Should flag nesting > 4");
    assert_eq!(summary.nesting_depth_warnings, 1);
}

#[test]
fn test_nesting_depth_at_threshold_no_warning() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        max_nesting: 4,
        ..Default::default()
    })];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(!results[0].nesting_depth_warning, "4 == threshold, no warn");
}

#[test]
fn test_function_length_warning_set() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        function_lines: 61,
        ..Default::default()
    })];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(results[0].function_length_warning, "Should flag >60 lines");
    assert_eq!(summary.function_length_warnings, 1);
}

#[test]
fn test_function_length_at_threshold_no_warning() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        function_lines: 60,
        ..Default::default()
    })];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(
        !results[0].function_length_warning,
        "60 == threshold, no warn"
    );
}

#[test]
fn test_unsafe_warning_set() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        unsafe_blocks: 1,
        ..Default::default()
    })];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(results[0].unsafe_warning, "Should flag unsafe blocks");
    assert_eq!(summary.unsafe_warnings, 1);
}

#[test]
fn test_error_handling_unwrap_warning() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        unwrap_count: 1,
        ..Default::default()
    })];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(results[0].error_handling_warning, "Should flag unwrap");
    assert_eq!(summary.error_handling_warnings, 1);
}

#[test]
fn test_error_handling_expect_allowed() {
    let mut config = Config::default();
    config.complexity.allow_expect = true;
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        expect_count: 3,
        ..Default::default()
    })];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(
        !results[0].error_handling_warning,
        "expect allowed, no warn"
    );
}

#[test]
fn test_error_handling_expect_not_allowed() {
    let mut config = Config::default();
    config.complexity.allow_expect = false;
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        expect_count: 1,
        ..Default::default()
    })];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(
        results[0].error_handling_warning,
        "expect not allowed, should warn"
    );
}

#[test]
fn test_suppressed_functions_skipped() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut func = make_func_with_metrics(ComplexityMetrics {
        max_nesting: 10,
        function_lines: 100,
        unsafe_blocks: 3,
        unwrap_count: 5,
        ..Default::default()
    });
    func.suppressed = true;
    let mut results = vec![func];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(!results[0].nesting_depth_warning);
    assert!(!results[0].function_length_warning);
    assert!(!results[0].unsafe_warning);
    assert!(!results[0].error_handling_warning);
}

#[test]
fn test_complexity_suppressed_functions_skipped() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut func = make_func_with_metrics(ComplexityMetrics {
        max_nesting: 10,
        function_lines: 100,
        ..Default::default()
    });
    func.complexity_suppressed = true;
    let mut results = vec![func];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(!results[0].nesting_depth_warning);
    assert!(!results[0].function_length_warning);
}

// ── exclude_test_violations ──────────────────────────────────

#[test]
fn test_exclude_test_violations_reclassifies() {
    let mut fa = make_func_with_metrics(ComplexityMetrics::default());
    fa.is_test = true;
    fa.classification = Classification::Violation {
        has_logic: true,
        has_own_calls: true,
        logic_locations: vec![],
        call_locations: vec![],
    };
    fa.severity = Some(crate::adapters::analyzers::iosp::Severity::Low);
    fa.effort_score = Some(3.0);
    let mut results = vec![fa];
    exclude_test_violations(&mut results);
    assert_eq!(results[0].classification, Classification::Trivial);
    assert!(results[0].severity.is_none());
    assert!(results[0].effort_score.is_none());
}

#[test]
fn test_exclude_test_violations_keeps_non_test() {
    let mut fa = make_func_with_metrics(ComplexityMetrics::default());
    fa.is_test = false;
    fa.classification = Classification::Violation {
        has_logic: true,
        has_own_calls: true,
        logic_locations: vec![],
        call_locations: vec![],
    };
    let mut results = vec![fa];
    exclude_test_violations(&mut results);
    assert!(matches!(
        results[0].classification,
        Classification::Violation { .. }
    ));
}

// ── error handling skipped for tests ─────────────────────────

#[test]
fn test_error_handling_skipped_for_test_fn() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut fa = make_func_with_metrics(ComplexityMetrics {
        unwrap_count: 3,
        ..Default::default()
    });
    fa.is_test = true;
    let mut results = vec![fa];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(!results[0].error_handling_warning);
    assert_eq!(summary.error_handling_warnings, 0);
}

#[test]
fn test_error_handling_flagged_for_non_test_fn() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut fa = make_func_with_metrics(ComplexityMetrics {
        unwrap_count: 1,
        ..Default::default()
    });
    fa.is_test = false;
    let mut results = vec![fa];
    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(results[0].error_handling_warning);
    assert_eq!(summary.error_handling_warnings, 1);
}

#[test]
fn test_unsafe_suppressed_by_allow_annotation() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut fa = make_func_with_metrics(ComplexityMetrics {
        unsafe_blocks: 1,
        ..Default::default()
    });
    fa.line = 5;
    let mut results = vec![fa];

    // qual:allow(unsafe) on line 4 (one line before fn at line 5)
    let unsafe_lines: HashMap<String, HashSet<usize>> =
        [("test.rs".to_string(), [4].into_iter().collect())].into();

    apply_extended_warnings(&mut results, &config, &mut summary, &unsafe_lines);
    assert!(
        !results[0].unsafe_warning,
        "qual:allow(unsafe) should suppress unsafe warning"
    );
    assert_eq!(summary.unsafe_warnings, 0);
}

#[test]
fn test_unsafe_without_allow_still_warned() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        unsafe_blocks: 1,
        ..Default::default()
    })];

    apply_extended_warnings(&mut results, &config, &mut summary, &HashMap::new());
    assert!(
        results[0].unsafe_warning,
        "Without annotation, unsafe should still warn"
    );
    assert_eq!(summary.unsafe_warnings, 1);
}

// ── detect_orphan_suppressions ─────────────────────────────────

fn empty_analysis() -> crate::report::AnalysisResult {
    crate::report::AnalysisResult {
        results: vec![],
        summary: Summary::default(),
        findings: crate::domain::AnalysisFindings::default(),
        data: crate::domain::AnalysisData::default(),
    }
}

fn make_finding(
    file: &str,
    line: usize,
    dimension: crate::findings::Dimension,
    rule_id: &str,
) -> crate::domain::Finding {
    crate::domain::Finding {
        file: file.into(),
        line,
        column: 0,
        dimension,
        rule_id: rule_id.into(),
        message: String::new(),
        severity: crate::domain::Severity::Medium,
        suppressed: false,
    }
}

fn make_srp_struct_finding(file: &str, line: usize) -> crate::domain::findings::SrpFinding {
    crate::domain::findings::SrpFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::Srp,
            "srp/struct_cohesion",
        ),
        kind: crate::domain::findings::SrpFindingKind::StructCohesion,
        details: crate::domain::findings::SrpFindingDetails::StructCohesion {
            struct_name: "Foo".into(),
            lcom4: 3,
            field_count: 5,
            method_count: 5,
            fan_out: 2,
        },
    }
}

fn make_srp_module_finding(file: &str) -> crate::domain::findings::SrpFinding {
    crate::domain::findings::SrpFinding {
        common: make_finding(
            file,
            1,
            crate::findings::Dimension::Srp,
            "srp/module_length",
        ),
        kind: crate::domain::findings::SrpFindingKind::ModuleLength,
        details: crate::domain::findings::SrpFindingDetails::ModuleLength {
            module: file.into(),
            production_lines: 900,
            independent_clusters: 1,
            cluster_names: vec![],
        },
    }
}

fn make_srp_param_finding(
    file: &str,
    line: usize,
    suppressed: bool,
) -> crate::domain::findings::SrpFinding {
    let mut common = make_finding(
        file,
        line,
        crate::findings::Dimension::Srp,
        "srp/parameter_count",
    );
    common.suppressed = suppressed;
    crate::domain::findings::SrpFinding {
        common,
        kind: crate::domain::findings::SrpFindingKind::ParameterCount,
        details: crate::domain::findings::SrpFindingDetails::ParameterCount {
            function_name: "big_factory".into(),
            parameter_count: 7,
        },
    }
}

fn make_tq_finding(
    file: &str,
    line: usize,
    kind: crate::domain::findings::TqFindingKind,
) -> crate::domain::findings::TqFinding {
    crate::domain::findings::TqFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::TestQuality,
            "tq/no_assertion",
        ),
        kind,
        function_name: "test_it".into(),
        uncovered_lines: None,
    }
}

fn make_dry_dead_code_finding(file: &str, line: usize) -> crate::domain::findings::DryFinding {
    crate::domain::findings::DryFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::Dry,
            "dry/dead_code/uncalled",
        ),
        kind: crate::domain::findings::DryFindingKind::DeadCodeUncalled,
        details: crate::domain::findings::DryFindingDetails::DeadCode {
            qualified_name: "helper".into(),
            suggestion: None,
        },
    }
}

fn make_dry_wildcard_finding(file: &str, line: usize) -> crate::domain::findings::DryFinding {
    crate::domain::findings::DryFinding {
        common: make_finding(file, line, crate::findings::Dimension::Dry, "dry/wildcard"),
        kind: crate::domain::findings::DryFindingKind::Wildcard,
        details: crate::domain::findings::DryFindingDetails::Wildcard {
            module_path: "foo::*".into(),
        },
    }
}

fn make_structural_srp_finding(file: &str, line: usize) -> crate::domain::findings::SrpFinding {
    crate::domain::findings::SrpFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::Srp,
            "srp/structural/SLM",
        ),
        kind: crate::domain::findings::SrpFindingKind::Structural,
        details: crate::domain::findings::SrpFindingDetails::Structural {
            item_name: "Foo::bar".into(),
            code: "SLM".into(),
            detail: "selfless method".into(),
        },
    }
}

fn make_structural_coupling_finding(
    file: &str,
    line: usize,
) -> crate::domain::findings::CouplingFinding {
    crate::domain::findings::CouplingFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::Coupling,
            "coupling/structural/SIT",
        ),
        kind: crate::domain::findings::CouplingFindingKind::Structural,
        details: crate::domain::findings::CouplingFindingDetails::Structural {
            item_name: "Foo".into(),
            code: "SIT".into(),
            detail: "single-impl trait".into(),
        },
    }
}

fn make_architecture_finding(
    file: &str,
    line: usize,
) -> crate::domain::findings::ArchitectureFinding {
    crate::domain::findings::ArchitectureFinding {
        common: make_finding(
            file,
            line,
            crate::findings::Dimension::Architecture,
            "architecture::layer",
        ),
    }
}

#[test]
fn orphan_suppression_without_matching_finding_is_counted() {
    // Suppression marker at line 5 with no finding in the window:
    // this is an orphan and must be counted.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Srp],
            reason: None,
        }],
    );
    let analysis = empty_analysis();
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    )
    .len();
    assert_eq!(orphans, 1, "unmatched marker should count as orphan");
}

#[test]
fn suppression_covering_finding_in_window_is_not_orphan() {
    // SRP struct finding at line 8; suppression marker at line 5 with
    // ANNOTATION_WINDOW=3 reaches line 8. Must NOT be orphan.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Srp],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_srp_struct_finding("src/foo.rs", 8));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    )
    .len();
    assert_eq!(orphans, 0, "in-window finding matches the marker");
}

#[test]
fn suppression_with_wrong_dimension_is_orphan() {
    // Finding is SRP, but marker suppresses only DRY → no dimension
    // match → orphan.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Dry],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_srp_struct_finding("src/foo.rs", 7));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    )
    .len();
    assert_eq!(orphans, 1, "dimension mismatch should still flag as orphan");
}

#[test]
fn bare_qual_allow_is_wildcard_and_matches_any_dim() {
    // Suppression has empty dimensions (bare `// qual:allow`) → matches
    // any dimension. A finding in window must clear the orphan.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_srp_struct_finding("src/foo.rs", 6));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    )
    .len();
    assert_eq!(orphans, 0, "bare qual:allow is wildcard");
}

// ── Regression tests: no false-positive orphans when the marker ──
// ── clears warning flags via suppression.                       ──
//
// These tests reproduce the Bug 3 iteration where my first orphan
// checker read `fa.cognitive_warning` and friends — flags that
// `apply_file_suppressions` clears when `// qual:allow(complexity)`
// matches. The checker then saw no position and flagged the marker
// as orphan, even though it was actively doing its job. The fixed
// checker reads raw `complexity` metrics against config thresholds,
// independent of the suppression flags.

fn make_fa_with_complexity(
    file: &str,
    line: usize,
    metrics: crate::adapters::analyzers::iosp::ComplexityMetrics,
) -> FunctionAnalysis {
    FunctionAnalysis {
        name: "f".into(),
        qualified_name: "f".into(),
        file: file.into(),
        line,
        classification: Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: Some(metrics),
        severity: None,
        cognitive_warning: false,
        cyclomatic_warning: false,
        nesting_depth_warning: false,
        function_length_warning: false,
        unsafe_warning: false,
        error_handling_warning: false,
        complexity_suppressed: true,
        own_calls: vec![],
        parameter_count: 0,
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
    }
}

#[test]
fn suppressed_cognitive_over_threshold_is_not_orphan() {
    // `qual:allow(complexity)` cleared cognitive_warning but the raw
    // metric still exceeds max_cognitive — marker is not orphan.
    use crate::adapters::analyzers::iosp::ComplexityMetrics;
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            cognitive_complexity: 99,
            ..Default::default()
        },
    )];
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "complexity marker clearing cognitive flag must not be orphan, got: {orphans:?}"
    );
}

#[test]
fn suppressed_cyclomatic_over_threshold_is_not_orphan() {
    use crate::adapters::analyzers::iosp::ComplexityMetrics;
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            cyclomatic_complexity: 99,
            ..Default::default()
        },
    )];
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(orphans.is_empty(), "got: {orphans:?}");
}

#[test]
fn suppressed_function_length_over_threshold_is_not_orphan() {
    use crate::adapters::analyzers::iosp::ComplexityMetrics;
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            function_lines: 200,
            ..Default::default()
        },
    )];
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(orphans.is_empty(), "got: {orphans:?}");
}

#[test]
fn suppressed_nesting_over_threshold_is_not_orphan() {
    use crate::adapters::analyzers::iosp::ComplexityMetrics;
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            max_nesting: 10,
            ..Default::default()
        },
    )];
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(orphans.is_empty(), "got: {orphans:?}");
}

#[test]
fn suppressed_unsafe_block_is_not_orphan() {
    use crate::adapters::analyzers::iosp::ComplexityMetrics;
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            unsafe_blocks: 1,
            ..Default::default()
        },
    )];
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(orphans.is_empty(), "got: {orphans:?}");
}

#[test]
fn suppressed_error_handling_unwrap_is_not_orphan() {
    use crate::adapters::analyzers::iosp::ComplexityMetrics;
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            unwrap_count: 3,
            ..Default::default()
        },
    )];
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(orphans.is_empty(), "got: {orphans:?}");
}

#[test]
fn suppressed_magic_number_is_not_orphan() {
    use crate::adapters::analyzers::iosp::{ComplexityMetrics, MagicNumberOccurrence};
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 10,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            magic_numbers: vec![MagicNumberOccurrence {
                line: 12,
                value: "42".into(),
            }],
            ..Default::default()
        },
    )];
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(orphans.is_empty(), "got: {orphans:?}");
}

#[test]
fn suppressed_srp_param_over_threshold_is_not_orphan() {
    // A `// qual:allow(srp)` marker on a function with >5 parameters:
    // `apply_parameter_warnings` now records the warning with
    // suppressed=true (it used to filter them out), so the orphan
    // checker finds a matching SRP position.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Srp],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_srp_param_finding("src/x.rs", 6, true));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "SRP param marker must match even on suppressed warnings, got: {orphans:?}"
    );
}

#[test]
fn coupling_marker_is_not_orphan_for_structural_coupling_finding() {
    // Structural binary checks (OI, SIT, DEH, IET) carry
    // `dimension == Coupling` and are line-anchored — a 5-line
    // qual:allow(coupling) window DOES suppress them. The orphan
    // checker must treat coupling-only markers as verifiable when a
    // line-anchored coupling position is available in the file.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 10,
            dimensions: vec![crate::findings::Dimension::Coupling],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .coupling
        .push(make_structural_coupling_finding("src/foo.rs", 12));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "coupling marker for a line-anchored structural finding must not be orphan, got: {orphans:?}"
    );
}

#[test]
fn coupling_only_marker_with_no_line_anchored_finding_is_skipped() {
    // When the file has no line-anchored Coupling position, a
    // coupling-only marker is unverifiable (pure module-level
    // coupling is global). We skip it rather than emit a
    // potentially-false orphan.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Coupling],
            reason: None,
        }],
    );
    let analysis = empty_analysis();
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "coupling-only marker without a line-anchored Coupling finding must be skipped, got: {orphans:?}"
    );
}

#[test]
fn dry_marker_on_dead_code_only_is_orphan() {
    // `qual:allow(dry)` does NOT suppress dead-code warnings (there
    // is no mark_dead_code_suppressions pass — exclusions happen via
    // qual:api / qual:test_helper / is_test / has_allow_dead_code).
    // So a qual:allow(dry) marker whose only nearby finding is
    // DEAD_CODE should be reported as orphan.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Dry],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .dry
        .push(make_dry_dead_code_finding("src/foo.rs", 7));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert_eq!(
        orphans.len(),
        1,
        "dry marker near dead_code only must be orphan (dead_code not dry-suppressible), got: {orphans:?}"
    );
}

#[test]
fn dry_marker_two_lines_above_wildcard_is_orphan() {
    // `mark_wildcard_suppressions` accepts markers only on the same
    // line or one line above. A marker two lines above the wildcard
    // would NOT suppress it, so the orphan checker must report it.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Dry],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .dry
        .push(make_dry_wildcard_finding("src/foo.rs", 7));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert_eq!(
        orphans.len(),
        1,
        "dry marker two lines above wildcard must be orphan (wildcard window is 1), got: {orphans:?}"
    );
}

#[test]
fn dry_marker_one_line_above_wildcard_is_not_orphan() {
    // Guard: the wildcard window is exactly 0 or 1 — same line or
    // line above. 1-line distance must still count as non-orphan.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 6,
            dimensions: vec![crate::findings::Dimension::Dry],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .dry
        .push(make_dry_wildcard_finding("src/foo.rs", 7));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "dry marker one line above wildcard must not be orphan, got: {orphans:?}"
    );
}

#[test]
fn complexity_marker_is_orphan_when_complexity_dimension_disabled() {
    // Config disables the complexity dimension entirely. A
    // `qual:allow(complexity)` marker can't suppress what doesn't
    // run, so it IS orphan even on a function with over-threshold
    // metrics.
    use crate::adapters::analyzers::iosp::ComplexityMetrics;
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            cognitive_complexity: 99,
            ..Default::default()
        },
    )];
    let mut config = Config::default();
    config.complexity.enabled = false;
    let orphans =
        crate::app::orphan_suppressions::detect_orphan_suppressions(&sups, &analysis, &config);
    assert_eq!(
        orphans.len(),
        1,
        "complexity marker with dimension disabled must be orphan"
    );
}

#[test]
fn srp_marker_is_orphan_when_srp_dimension_disabled() {
    // Same pattern for SRP: disable the dimension, a qual:allow(srp)
    // can't suppress anything → orphan even if a SrpWarning is in
    // the analysis struct (stale from a previous run, or leftover).
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 2,
            dimensions: vec![crate::findings::Dimension::Srp],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_srp_struct_finding("src/foo.rs", 7));
    let mut config = Config::default();
    config.srp.enabled = false;
    let orphans =
        crate::app::orphan_suppressions::detect_orphan_suppressions(&sups, &analysis, &config);
    assert_eq!(
        orphans.len(),
        1,
        "SRP marker with dimension disabled must be orphan"
    );
}

#[test]
fn srp_struct_marker_within_5_line_window_is_not_orphan() {
    // SRP struct suppressions use SRP_STRUCT_SUPPRESSION_WINDOW=5
    // (wider than ANNOTATION_WINDOW=3) because #[derive(...)]
    // attributes can push the marker further from the struct. A
    // marker at line 2 matching a struct at line 7 (diff=5) must not
    // be flagged as orphan.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 2,
            dimensions: vec![crate::findings::Dimension::Srp],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_srp_struct_finding("src/foo.rs", 7));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "SRP struct marker within the 5-line window must not be orphan, got: {orphans:?}"
    );
}

#[test]
fn srp_module_marker_anywhere_in_file_is_not_orphan() {
    // SRP module warnings are suppressed file-globally by
    // `mark_srp_suppressions` — any qual:allow(srp) anywhere in the
    // file matches. The orphan checker must not require line
    // proximity for module-level SRP findings.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/big.rs".to_string(),
        vec![Suppression {
            line: 500,
            dimensions: vec![crate::findings::Dimension::Srp],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_srp_module_finding("src/big.rs"));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "SRP module marker at any line must match the file-global module finding, got: {orphans:?}"
    );
}

#[test]
fn tq_marker_within_5_line_window_is_not_orphan() {
    // TQ suppressions use a 5-line window (mark_tq_suppressions).
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 10,
            dimensions: vec![crate::findings::Dimension::TestQuality],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.findings.test_quality.push(make_tq_finding(
        "src/foo.rs",
        15,
        crate::domain::findings::TqFindingKind::NoAssertion,
    ));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "TQ marker within 5-line window must not be orphan, got: {orphans:?}"
    );
}

#[test]
fn structural_marker_within_5_line_window_is_not_orphan() {
    // Structural binary checks use a 5-line window
    // (mark_structural_suppressions).
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 10,
            dimensions: vec![crate::findings::Dimension::Srp],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .srp
        .push(make_structural_srp_finding("src/foo.rs", 15));
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert!(
        orphans.is_empty(),
        "Structural marker within 5-line window must not be orphan, got: {orphans:?}"
    );
}

#[test]
fn architecture_marker_only_matches_findings_in_window() {
    // Architecture suppressions must be scoped to the marker's
    // annotation window, not the whole file. A `qual:allow(architecture)`
    // for one helper must not silence unrelated layer / forbidden /
    // call-parity findings elsewhere in the same file, and the orphan
    // checker must report a stale marker whose only architecture
    // finding lives outside the window.
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/foo.rs".to_string(),
        vec![Suppression {
            line: 1,
            dimensions: vec![crate::findings::Dimension::Architecture],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis
        .findings
        .architecture
        .push(make_architecture_finding("src/foo.rs", 500));
    let mut config = Config::default();
    config.architecture.enabled = true;
    let orphans =
        crate::app::orphan_suppressions::detect_orphan_suppressions(&sups, &analysis, &config);
    assert!(
        !orphans.is_empty(),
        "Architecture marker at line 1 with only a finding at line 500 must \
         be reported as orphan (window-scoped, not file-scoped); got: {orphans:?}"
    );
}

#[test]
fn complexity_marker_without_any_overshoot_is_orphan() {
    // Sanity: if a marker truly has no target — all complexity metrics
    // are within limits — it IS orphan.
    use crate::adapters::analyzers::iosp::ComplexityMetrics;
    use crate::findings::Suppression;
    let mut sups = HashMap::new();
    sups.insert(
        "src/x.rs".to_string(),
        vec![Suppression {
            line: 5,
            dimensions: vec![crate::findings::Dimension::Complexity],
            reason: None,
        }],
    );
    let mut analysis = empty_analysis();
    analysis.results = vec![make_fa_with_complexity(
        "src/x.rs",
        6,
        ComplexityMetrics {
            cognitive_complexity: 1,
            cyclomatic_complexity: 1,
            function_lines: 5,
            ..Default::default()
        },
    )];
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &sups,
        &analysis,
        &Config::default(),
    );
    assert_eq!(
        orphans.len(),
        1,
        "marker with no over-threshold target must be orphan"
    );
}
