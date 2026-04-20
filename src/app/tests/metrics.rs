use crate::adapters::analyzers::iosp::{compute_severity, Classification, FunctionAnalysis};
use crate::app::dry_suppressions::{mark_dry_suppressions, mark_inverse_suppressions};
use crate::app::metrics::*;
use crate::config::sections::SrpConfig;
use crate::findings::Suppression;
use crate::report::Summary;

fn make_func(name: &str, param_count: usize, trait_impl: bool) -> FunctionAnalysis {
    let severity = compute_severity(&Classification::Operation);
    FunctionAnalysis {
        name: name.to_string(),
        file: "test.rs".to_string(),
        line: 1,
        classification: Classification::Operation,
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
        parameter_count: param_count,
        is_trait_impl: trait_impl,
        is_test: false,
        effort_score: None,
    }
}

fn make_srp() -> crate::adapters::analyzers::srp::SrpAnalysis {
    crate::adapters::analyzers::srp::SrpAnalysis {
        struct_warnings: vec![],
        module_warnings: vec![],
        param_warnings: vec![],
    }
}

#[test]
fn test_param_warning_exceeds_threshold() {
    let config = SrpConfig::default();
    let results = vec![make_func("many_params", 7, false)];
    let mut srp = make_srp();
    apply_parameter_warnings(&results, Some(&mut srp), &config);
    assert_eq!(srp.param_warnings.len(), 1);
    assert_eq!(srp.param_warnings[0].parameter_count, 7);
    assert_eq!(srp.param_warnings[0].function_name, "many_params");
}

#[test]
fn test_param_warning_at_threshold_no_warning() {
    let config = SrpConfig::default();
    let results = vec![make_func("ok_params", 5, false)];
    let mut srp = make_srp();
    apply_parameter_warnings(&results, Some(&mut srp), &config);
    assert!(srp.param_warnings.is_empty(), "5 == threshold, no warning");
}

#[test]
fn test_param_warning_trait_impl_excluded() {
    let config = SrpConfig::default();
    let results = vec![make_func("trait_fn", 10, true)];
    let mut srp = make_srp();
    apply_parameter_warnings(&results, Some(&mut srp), &config);
    assert!(
        srp.param_warnings.is_empty(),
        "trait impl should be excluded"
    );
}

#[test]
fn test_param_warning_suppressed_fn_is_flagged_but_marked_suppressed() {
    // Suppressed over-threshold functions now emit a warning with
    // `suppressed=true` instead of being filtered out silently. This
    // lets the orphan-suppression checker see that a `qual:allow(srp)`
    // marker did have a target. `summary.srp_param_warnings` still
    // counts only non-suppressed entries, so user-visible behavior
    // (finding count, quality score) is unchanged.
    let config = SrpConfig::default();
    let mut func = make_func("suppressed_fn", 10, false);
    func.suppressed = true;
    let results = vec![func];
    let mut srp = make_srp();
    apply_parameter_warnings(&results, Some(&mut srp), &config);
    assert_eq!(srp.param_warnings.len(), 1, "entry recorded");
    assert!(
        srp.param_warnings[0].suppressed,
        "entry must be marked suppressed"
    );
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn test_param_warning_custom_threshold() {
    let mut config = SrpConfig::default();
    config.max_parameters = 3;
    let results = vec![make_func("four_params", 4, false)];
    let mut srp = make_srp();
    apply_parameter_warnings(&results, Some(&mut srp), &config);
    assert_eq!(srp.param_warnings.len(), 1, "4 > custom threshold 3");
}

#[test]
fn test_param_warning_srp_none() {
    let config = SrpConfig::default();
    let results = vec![make_func("fn", 10, false)];
    apply_parameter_warnings(&results, None, &config);
    // No panic, no-op when SRP is None
}

// ── SDP suppression tests ──────────────────────────────

#[test]
fn test_count_sdp_violations_excludes_suppressed() {
    let analysis = crate::adapters::analyzers::coupling::CouplingAnalysis {
        metrics: vec![],
        cycles: vec![],
        sdp_violations: vec![
            crate::adapters::analyzers::coupling::sdp::SdpViolation {
                from_module: "a".into(),
                to_module: "b".into(),
                from_instability: 0.2,
                to_instability: 0.8,
                suppressed: true,
            },
            crate::adapters::analyzers::coupling::sdp::SdpViolation {
                from_module: "c".into(),
                to_module: "d".into(),
                from_instability: 0.3,
                to_instability: 0.9,
                suppressed: false,
            },
        ],
        graph: crate::adapters::analyzers::coupling::ModuleGraph::default(),
    };
    let config = crate::config::sections::CouplingConfig::default();
    let mut summary = Summary::from_results(&[]);
    count_sdp_violations(Some(&analysis), &config, &mut summary);
    assert_eq!(
        summary.sdp_violations, 1,
        "Only unsuppressed violations counted"
    );
}

#[test]
fn test_mark_dry_suppressions() {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };

    let mut groups = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "as_str".to_string(),
                qualified_name: "Foo::as_str".to_string(),
                file: "test.rs".to_string(),
                line: 5,
            },
            DuplicateEntry {
                name: "parse".to_string(),
                qualified_name: "Foo::parse".to_string(),
                file: "test.rs".to_string(),
                line: 15,
            },
        ],
        kind: DuplicateKind::NearDuplicate { similarity: 0.91 },
        suppressed: false,
    }];

    // Suppression on line 4 (one line before as_str at line 5) with dry dimension
    let sup = Suppression {
        line: 4,
        dimensions: vec![crate::findings::Dimension::Dry],
        reason: None,
    };
    let suppression_lines: std::collections::HashMap<String, Vec<Suppression>> =
        [("test.rs".to_string(), vec![sup])].into();

    mark_dry_suppressions(&mut groups, &suppression_lines);
    assert!(
        groups[0].suppressed,
        "Group should be suppressed when any member has qual:allow(dry)"
    );
}

#[test]
fn test_duplicate_without_suppression_not_marked() {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };

    let mut groups = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "foo".to_string(),
                qualified_name: "foo".to_string(),
                file: "test.rs".to_string(),
                line: 5,
            },
            DuplicateEntry {
                name: "bar".to_string(),
                qualified_name: "bar".to_string(),
                file: "test.rs".to_string(),
                line: 15,
            },
        ],
        kind: DuplicateKind::Exact,
        suppressed: false,
    }];

    let suppression_lines: std::collections::HashMap<String, Vec<Suppression>> =
        std::collections::HashMap::new();

    mark_dry_suppressions(&mut groups, &suppression_lines);
    assert!(
        !groups[0].suppressed,
        "Group without suppression should not be marked"
    );
}

#[test]
fn test_inverse_annotation_suppresses_duplicate() {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };

    let mut groups = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "as_str".to_string(),
                qualified_name: "Foo::as_str".to_string(),
                file: "test.rs".to_string(),
                line: 5,
            },
            DuplicateEntry {
                name: "parse".to_string(),
                qualified_name: "Foo::parse".to_string(),
                file: "test.rs".to_string(),
                line: 15,
            },
        ],
        kind: DuplicateKind::NearDuplicate { similarity: 0.91 },
        suppressed: false,
    }];

    // qual:inverse(parse) on line 4 (one before as_str at line 5)
    let inverse_lines: std::collections::HashMap<String, Vec<(usize, String)>> =
        [("test.rs".to_string(), vec![(4, "parse".to_string())])].into();

    mark_inverse_suppressions(&mut groups, &inverse_lines);
    assert!(
        groups[0].suppressed,
        "Inverse-annotated pair should be suppressed"
    );
}

#[test]
fn test_inverse_annotation_must_target_group_member() {
    use crate::adapters::analyzers::dry::functions::{
        DuplicateEntry, DuplicateGroup, DuplicateKind,
    };

    let mut groups = vec![DuplicateGroup {
        entries: vec![
            DuplicateEntry {
                name: "foo".to_string(),
                qualified_name: "foo".to_string(),
                file: "test.rs".to_string(),
                line: 5,
            },
            DuplicateEntry {
                name: "bar".to_string(),
                qualified_name: "bar".to_string(),
                file: "test.rs".to_string(),
                line: 15,
            },
        ],
        kind: DuplicateKind::Exact,
        suppressed: false,
    }];

    // qual:inverse(baz) targets a function not in the group
    let inverse_lines: std::collections::HashMap<String, Vec<(usize, String)>> =
        [("test.rs".to_string(), vec![(4, "baz".to_string())])].into();

    mark_inverse_suppressions(&mut groups, &inverse_lines);
    assert!(
        !groups[0].suppressed,
        "Inverse targeting non-member should not suppress"
    );
}

#[test]
fn test_repeated_match_suppression() {
    use crate::adapters::analyzers::dry::match_patterns::{RepeatedMatchEntry, RepeatedMatchGroup};

    let mut groups = vec![RepeatedMatchGroup {
        enum_name: "MyEnum".to_string(),
        entries: vec![RepeatedMatchEntry {
            file: "test.rs".to_string(),
            line: 10,
            function_name: "handle_a".to_string(),
            arm_count: 5,
        }],
        suppressed: false,
    }];

    let sup = Suppression {
        line: 9,
        dimensions: vec![crate::findings::Dimension::Dry],
        reason: None,
    };
    let suppression_lines: std::collections::HashMap<String, Vec<Suppression>> =
        [("test.rs".to_string(), vec![sup])].into();

    mark_dry_suppressions(&mut groups, &suppression_lines);
    assert!(
        groups[0].suppressed,
        "RepeatedMatchGroup should be suppressed by qual:allow(dry)"
    );
}

#[test]
fn test_fragment_suppression() {
    use crate::adapters::analyzers::dry::fragments::{FragmentEntry, FragmentGroup};

    let mut groups = vec![FragmentGroup {
        entries: vec![FragmentEntry {
            function_name: "foo".to_string(),
            qualified_name: "foo".to_string(),
            file: "test.rs".to_string(),
            start_line: 5,
            end_line: 10,
        }],
        statement_count: 3,
        suppressed: false,
    }];

    let sup = Suppression {
        line: 4,
        dimensions: vec![crate::findings::Dimension::Dry],
        reason: None,
    };
    let suppression_lines: std::collections::HashMap<String, Vec<Suppression>> =
        [("test.rs".to_string(), vec![sup])].into();

    mark_dry_suppressions(&mut groups, &suppression_lines);
    assert!(
        groups[0].suppressed,
        "FragmentGroup should be suppressed by qual:allow(dry)"
    );
}

#[test]
fn test_boilerplate_suppression() {
    use crate::adapters::analyzers::dry::boilerplate::BoilerplateFind;

    let mut findings = vec![BoilerplateFind {
        pattern_id: "BP-003".to_string(),
        file: "test.rs".to_string(),
        line: 10,
        struct_name: Some("MyStruct".to_string()),
        description: "3 trivial getters".to_string(),
        suggestion: "Consider derive macro".to_string(),
        suppressed: false,
    }];

    let sup = Suppression {
        line: 9,
        dimensions: vec![crate::findings::Dimension::Dry],
        reason: None,
    };
    let suppression_lines: std::collections::HashMap<String, Vec<Suppression>> =
        [("test.rs".to_string(), vec![sup])].into();

    mark_dry_suppressions(&mut findings, &suppression_lines);
    assert!(
        findings[0].suppressed,
        "BoilerplateFind should be suppressed by qual:allow(dry)"
    );
}
