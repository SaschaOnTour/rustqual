use crate::adapters::analyzers::iosp::Classification;
use crate::adapters::source::filesystem::{
    collect_filtered_files, collect_rust_files, collect_suppression_lines, read_and_parse_files,
};
use crate::app::coupling_suppressions::{mark_coupling_blanket, ModuleCouplingSuppressions};
use crate::app::metrics::count_coupling_warnings;
use crate::app::pipeline::{analyze_and_output, output_results, run_analysis};
use crate::app::warnings::{check_suppression_ratio, count_all_suppressions};
use crate::config::Config;
use crate::findings::Suppression;
use crate::report::{AnalysisResult, Summary};
use std::fs;

fn test_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("rustqual_test_")
        .tempdir()
        .unwrap()
}

#[test]
fn test_collect_rust_files_single_file() {
    let tmp = test_dir();
    let rs_file = tmp.path().join("test.rs");
    fs::write(&rs_file, "fn main() {}").unwrap();
    let files = collect_rust_files(&rs_file);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0], rs_file);
}

#[test]
fn test_collect_rust_files_non_rust_file() {
    let tmp = test_dir();
    let txt_file = tmp.path().join("test.txt");
    fs::write(&txt_file, "hello").unwrap();
    let files = collect_rust_files(&txt_file);
    assert!(files.is_empty());
}

#[test]
fn test_collect_rust_files_directory() {
    let tmp = test_dir();
    fs::write(tmp.path().join("a.rs"), "fn a() {}").unwrap();
    fs::write(tmp.path().join("b.rs"), "fn b() {}").unwrap();
    fs::write(tmp.path().join("c.txt"), "not rust").unwrap();
    let files = collect_rust_files(tmp.path());
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|f| f.extension().unwrap() == "rs"));
}

/// In a fresh tempdir, put a `.rs` inside `excluded_subdir` and a visible
/// `.rs` at the root, then collect — returning how many files were found.
fn collect_count_excluding(excluded_subdir: &str) -> usize {
    let tmp = test_dir();
    let sub = tmp.path().join(excluded_subdir);
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("excluded.rs"), "fn x() {}").unwrap();
    fs::write(tmp.path().join("visible.rs"), "fn v() {}").unwrap();
    collect_rust_files(tmp.path()).len()
}

#[test]
fn test_collect_rust_files_skips_target_and_hidden_dirs() {
    // `target/` and `.`-prefixed directories are excluded; only the visible
    // root file is collected.
    for excluded in ["target", ".hidden"] {
        assert_eq!(
            collect_count_excluding(excluded),
            1,
            "`{excluded}/` should be excluded, leaving only the visible file"
        );
    }
}

#[test]
fn test_collect_rust_files_empty_dir() {
    let tmp = test_dir();
    let files = collect_rust_files(tmp.path());
    assert!(files.is_empty());
}

#[test]
fn test_collect_filtered_files_no_exclude() {
    let tmp = test_dir();
    fs::write(tmp.path().join("a.rs"), "fn a() {}").unwrap();
    fs::write(tmp.path().join("b.rs"), "fn b() {}").unwrap();
    let config = Config::default();
    let files = collect_filtered_files(tmp.path(), &config);
    assert_eq!(files.len(), 2);
}

#[test]
fn test_collect_filtered_files_with_exclude() {
    let tmp = test_dir();
    let gen_dir = tmp.path().join("generated");
    fs::create_dir(&gen_dir).unwrap();
    fs::write(gen_dir.join("gen.rs"), "fn g() {}").unwrap();
    fs::write(tmp.path().join("main.rs"), "fn m() {}").unwrap();
    let mut config = Config::default();
    config.exclude_files = vec!["generated/**".into()];
    config.compile();
    let files = collect_filtered_files(tmp.path(), &config);
    assert_eq!(files.len(), 1);
}

// ── Suppression tests (new syntax) ──────────────────────────────

#[test]
fn test_collect_suppression_bare_qual_allow_is_ignored() {
    // Bare `// qual:allow` (no dimension) is silently dropped by the
    // parser — there's no global-suppress feature, every suppression
    // must target a named dimension.
    let source = "// qual:allow\nfn foo() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let result = collect_suppression_lines(&parsed);
    let sups = result.get("test.rs").cloned().unwrap_or_default();
    assert!(
        sups.is_empty(),
        "bare qual:allow must not produce a Suppression, got {sups:?}"
    );
}

#[test]
fn test_collect_suppression_qual_allow_iosp() {
    let source = "// qual:allow(iosp)\nfn foo() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let result = collect_suppression_lines(&parsed);
    let sups = &result["test.rs"];
    assert_eq!(sups[0].dimensions.len(), 1);
    assert_eq!(sups[0].dimensions[0], crate::findings::Dimension::Iosp);
}

#[test]
fn test_collect_suppression_qual_allow_with_reason() {
    let source = "// qual:allow(iosp) reason: \"syn pattern\"\nfn foo() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let result = collect_suppression_lines(&parsed);
    let sups = &result["test.rs"];
    assert_eq!(sups[0].reason.as_deref(), Some("syn pattern"));
}

#[test]
fn test_collect_suppression_old_iosp_allow_still_works() {
    let source = "// iosp:allow\nfn foo() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let result = collect_suppression_lines(&parsed);
    assert!(result.contains_key("test.rs"));
    let sups = &result["test.rs"];
    assert_eq!(sups[0].dimensions.len(), 1);
    assert_eq!(sups[0].dimensions[0], crate::findings::Dimension::Iosp);
}

#[test]
fn test_collect_suppression_old_iosp_allow_with_reason() {
    let source = "// iosp:allow justified reason\nfn foo() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let result = collect_suppression_lines(&parsed);
    assert!(result.contains_key("test.rs"));
}

#[test]
fn test_collect_suppression_no_match() {
    let source = "// normal comment\nfn foo() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let result = collect_suppression_lines(&parsed);
    assert!(result.is_empty());
}

#[test]
fn test_collect_suppression_multiple() {
    let source = "// qual:allow(srp, god_struct) reason: \"x\"\nfn foo() {}\n// qual:allow(iosp)\nfn bar() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let result = collect_suppression_lines(&parsed);
    assert!(result.contains_key("test.rs"));
    assert_eq!(result["test.rs"].len(), 2);
}

#[test]
fn test_run_analysis_empty_input() {
    let parsed: Vec<(String, String, syn::File)> = vec![];
    let config = Config::default();
    let analysis = run_analysis(parsed, &config);
    assert!(analysis.results.is_empty());
    assert_eq!(analysis.summary.total, 0);
}

#[test]
fn test_run_analysis_trivial_function() {
    let source = "fn empty() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let config = Config::default();
    let analysis = run_analysis(parsed, &config);
    assert_eq!(analysis.results.len(), 1);
    assert!(matches!(
        analysis.results[0].classification,
        Classification::Trivial
    ));
    assert_eq!(analysis.summary.trivial, 1);
}

#[test]
fn test_read_and_parse_files_valid() {
    let tmp = test_dir();
    let f1 = tmp.path().join("a.rs");
    let f2 = tmp.path().join("b.rs");
    fs::write(&f1, "fn a() {}").unwrap();
    fs::write(&f2, "fn b() { let x = 1; }").unwrap();
    let files = vec![f1, f2];
    let parsed = read_and_parse_files(&files, tmp.path());
    assert_eq!(parsed.len(), 2);
}

#[test]
fn test_read_and_parse_files_invalid_syntax() {
    let tmp = test_dir();
    let f = tmp.path().join("bad.rs");
    fs::write(&f, "fn broken( {}").unwrap();
    let files = vec![f];
    let parsed = read_and_parse_files(&files, tmp.path());
    assert!(parsed.is_empty(), "Invalid syntax should be skipped");
}

#[test]
fn test_read_and_parse_files_missing_file() {
    let tmp = test_dir();
    let f = tmp.path().join("nonexistent.rs");
    let files = vec![f];
    let parsed = read_and_parse_files(&files, tmp.path());
    assert!(parsed.is_empty(), "Missing file should be skipped");
}

#[test]
fn test_output_results_dispatches_text_format_on_empty_analysis() {
    // Pipeline dispatch test: `output_results` is a thin dispatcher
    // over OutputFormat. The data-transformation paths (per-reporter)
    // have their own value-asserting tests; here we only verify the
    // dispatch wires up without panicking and that the Text branch is
    // reachable for an empty analysis. (Output capture would require
    // redirecting stdout, which adds complexity without catching any
    // additional class of regression — every reporter has its own
    // value-asserting tests now.)
    let results = vec![];
    let summary = crate::report::Summary::from_results(&results);
    let analysis = AnalysisResult {
        results,
        summary,
        findings: crate::domain::AnalysisFindings::default(),
        data: crate::domain::AnalysisData::default(),
    };
    output_results(
        &analysis,
        &crate::OutputFormat::Text,
        false,
        false,
        &crate::config::Config::default(),
    );
}

// ── Coupling suppression tests ──────────────────────────────

fn make_coupling_analysis() -> crate::adapters::analyzers::coupling::CouplingAnalysis {
    crate::adapters::analyzers::coupling::CouplingAnalysis {
        metrics: vec![
            crate::adapters::analyzers::coupling::CouplingMetrics {
                module_name: "pipeline".to_string(),
                afferent: 1,
                efferent: 5,
                instability: 0.83,
                incoming: vec!["watch".to_string()],
                outgoing: vec![
                    "analyzer".to_string(),
                    "config".to_string(),
                    "findings".to_string(),
                    "report".to_string(),
                    "scope".to_string(),
                ],
                suppressed: false,
                warning: false,
            },
            crate::adapters::analyzers::coupling::CouplingMetrics {
                module_name: "config".to_string(),
                afferent: 3,
                efferent: 0,
                instability: 0.0,
                incoming: vec![
                    "analyzer".to_string(),
                    "pipeline".to_string(),
                    "watch".to_string(),
                ],
                outgoing: vec![],
                suppressed: false,
                warning: false,
            },
        ],
        cycles: vec![],
        sdp_violations: vec![],
        graph: crate::adapters::analyzers::coupling::ModuleGraph::default(),
    }
}

#[test]
fn test_mark_coupling_suppressions_marks_module() {
    let mut analysis = make_coupling_analysis();
    let sup = Suppression {
        line: 1,
        dimensions: vec![crate::findings::Dimension::Coupling],
        reason: Some("orchestrator module".to_string()),
        target: None,
    };
    let mut suppression_lines = std::collections::HashMap::new();
    suppression_lines.insert("pipeline.rs".to_string(), vec![sup]);

    mark_coupling_blanket(
        Some(&mut analysis),
        &ModuleCouplingSuppressions::build(&suppression_lines),
    );

    assert!(analysis.metrics[0].suppressed); // pipeline
    assert!(!analysis.metrics[1].suppressed); // config
}

/// Mark a `pipeline.rs`-line-1 suppression with `dims` against the standard
/// coupling analysis, and report whether its first metric got suppressed.
fn coupling_metric0_suppressed_by(dims: Vec<crate::findings::Dimension>) -> bool {
    let mut analysis = make_coupling_analysis();
    let mut suppression_lines = std::collections::HashMap::new();
    suppression_lines.insert(
        "pipeline.rs".to_string(),
        vec![Suppression {
            line: 1,
            dimensions: dims,
            reason: None,
            target: None,
        }],
    );
    mark_coupling_blanket(
        Some(&mut analysis),
        &ModuleCouplingSuppressions::build(&suppression_lines),
    );
    analysis.metrics[0].suppressed
}

#[test]
fn test_mark_coupling_suppressions_dimension_coverage() {
    use crate::findings::Dimension;
    // A wildcard (`qual:allow` with no dims) covers coupling; an unrelated
    // single dimension (iosp) does not. (label, dims, covers_coupling)
    let cases: &[(&str, &[Dimension], bool)] = &[
        ("wildcard (no dims) covers coupling", &[], true),
        (
            "iosp-only does not cover coupling",
            &[Dimension::Iosp],
            false,
        ),
    ];
    for (label, dims, covers) in cases {
        assert_eq!(
            coupling_metric0_suppressed_by(dims.to_vec()),
            *covers,
            "case {label}"
        );
    }
}

#[test]
fn test_mark_coupling_suppressions_submodule_file() {
    let mut analysis = crate::adapters::analyzers::coupling::CouplingAnalysis {
        metrics: vec![crate::adapters::analyzers::coupling::CouplingMetrics {
            module_name: "analyzer".to_string(),
            afferent: 3,
            efferent: 2,
            instability: 0.4,
            incoming: vec![],
            outgoing: vec![],
            suppressed: false,
            warning: false,
        }],
        cycles: vec![],
        sdp_violations: vec![],
        graph: crate::adapters::analyzers::coupling::ModuleGraph::default(),
    };
    let sup = Suppression {
        line: 1,
        dimensions: vec![crate::findings::Dimension::Coupling],
        reason: None,
        target: None,
    };
    let mut suppression_lines = std::collections::HashMap::new();
    // Suppression in a submodule file maps to the top-level module
    suppression_lines.insert("analyzer/visitor.rs".to_string(), vec![sup]);

    mark_coupling_blanket(
        Some(&mut analysis),
        &ModuleCouplingSuppressions::build(&suppression_lines),
    );

    assert!(analysis.metrics[0].suppressed); // analyzer suppressed
}

#[test]
fn test_mark_coupling_suppressions_none_analysis() {
    let suppression_lines = std::collections::HashMap::new();
    // Should not panic
    mark_coupling_blanket(None, &ModuleCouplingSuppressions::build(&suppression_lines));
}

#[test]
fn test_count_coupling_warnings_skips_suppressed() {
    let mut analysis = make_coupling_analysis();
    // A blanket allow(coupling) in a file of the `pipeline` module silences it.
    let mut suppression_lines = std::collections::HashMap::new();
    suppression_lines.insert(
        "pipeline.rs".to_string(),
        vec![Suppression {
            line: 1,
            dimensions: vec![crate::findings::Dimension::Coupling],
            reason: Some("orchestrator".to_string()),
            target: None,
        }],
    );

    let config = crate::config::sections::CouplingConfig::default();
    let mut summary = Summary::from_results(&[]);

    count_coupling_warnings(
        Some(&mut analysis),
        &config,
        &ModuleCouplingSuppressions::build(&suppression_lines),
        &mut summary,
    );

    assert_eq!(summary.coupling_warnings, 0); // pipeline warning suppressed
}

#[test]
fn test_count_coupling_warnings_counts_unsuppressed() {
    let mut analysis = make_coupling_analysis();

    let config = crate::config::sections::CouplingConfig::default();
    let mut summary = Summary::from_results(&[]);

    count_coupling_warnings(
        Some(&mut analysis),
        &config,
        &ModuleCouplingSuppressions::build(&std::collections::HashMap::new()),
        &mut summary,
    );

    assert_eq!(summary.coupling_warnings, 1); // pipeline exceeds threshold
}

#[test]
fn test_count_coupling_warnings_leaf_module_excluded() {
    let mut analysis = crate::adapters::analyzers::coupling::CouplingAnalysis {
        metrics: vec![crate::adapters::analyzers::coupling::CouplingMetrics {
            module_name: "watch".to_string(),
            afferent: 0, // leaf module
            efferent: 2,
            instability: 1.0,
            incoming: vec![],
            outgoing: vec!["config".to_string(), "pipeline".to_string()],
            suppressed: false,
            warning: false,
        }],
        cycles: vec![],
        sdp_violations: vec![],
        graph: crate::adapters::analyzers::coupling::ModuleGraph::default(),
    };

    let config = crate::config::sections::CouplingConfig::default();
    let mut summary = Summary::from_results(&[]);

    count_coupling_warnings(
        Some(&mut analysis),
        &config,
        &ModuleCouplingSuppressions::build(&std::collections::HashMap::new()),
        &mut summary,
    );

    assert_eq!(summary.coupling_warnings, 0); // leaf excluded
}

// ── Suppression ratio tests ──────────────────────────────

#[test]
fn test_check_suppression_ratio_below() {
    // 1 out of 100 = 1%, below 5% threshold
    assert!(!check_suppression_ratio(100, 1, 0.05));
}

#[test]
fn test_check_suppression_ratio_above() {
    // 10 out of 100 = 10%, above 5% threshold
    assert!(check_suppression_ratio(100, 10, 0.05));
}

#[test]
fn test_check_suppression_ratio_zero_total() {
    assert!(!check_suppression_ratio(0, 0, 0.05));
}

#[test]
fn test_check_suppression_ratio_at_boundary() {
    // 5 out of 100 = exactly 5%, not exceeded (not strictly greater)
    assert!(!check_suppression_ratio(100, 5, 0.05));
}

#[test]
fn test_check_suppression_ratio_just_above() {
    // 6 out of 100 = 6%, above 5%
    assert!(check_suppression_ratio(100, 6, 0.05));
}

#[test]
fn test_count_all_suppressions_qual_only() {
    let source = "// qual:allow\nfn foo() {}\n// qual:allow(iosp)\nfn bar() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let mut supp = std::collections::HashMap::new();
    supp.insert(
        "test.rs".to_string(),
        vec![
            crate::findings::Suppression {
                line: 1,
                dimensions: vec![],
                reason: None,
                target: None,
            },
            crate::findings::Suppression {
                line: 3,
                dimensions: vec![crate::findings::Dimension::Iosp],
                reason: None,
                target: None,
            },
        ],
    );
    assert_eq!(count_all_suppressions(&supp, &parsed), 2);
}

#[test]
fn test_count_all_suppressions_rust_allow_only() {
    let source = "#[allow(dead_code)]\nfn unused() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let supp = std::collections::HashMap::new();
    assert_eq!(count_all_suppressions(&supp, &parsed), 1);
}

#[test]
fn test_count_all_suppressions_both_types() {
    let source = "#[allow(dead_code)]\nfn unused() {}\n// qual:allow(iosp)\nfn foo() {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let mut supp = std::collections::HashMap::new();
    supp.insert(
        "test.rs".to_string(),
        vec![crate::findings::Suppression {
            line: 3,
            dimensions: vec![crate::findings::Dimension::Iosp],
            reason: None,
            target: None,
        }],
    );
    assert_eq!(count_all_suppressions(&supp, &parsed), 2);
}

#[test]
fn test_count_all_suppressions_test_code_excluded() {
    let source =
        "fn good() {}\n#[cfg(test)]\nmod tests {\n#[allow(dead_code)]\nfn test_helper() {}\n}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let supp = std::collections::HashMap::new();
    assert_eq!(count_all_suppressions(&supp, &parsed), 0);
}

#[test]
fn test_count_all_suppressions_allow_before_cfg_test_excluded() {
    // #[allow(dead_code)] directly before #[cfg(test)] is part of the test module
    let source = "#[allow(dead_code)]\n#[cfg(test)]\nmod tests {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let supp = std::collections::HashMap::new();
    assert_eq!(count_all_suppressions(&supp, &parsed), 0);
}

#[test]
fn test_count_all_suppressions_allow_with_gap_counted() {
    // #[allow(dead_code)] with a gap before #[cfg(test)] is production code
    let source = "#[allow(dead_code)]\nfn foo() {}\n#[cfg(test)]\nmod tests {}";
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let supp = std::collections::HashMap::new();
    assert_eq!(count_all_suppressions(&supp, &parsed), 1);
}

// ── Pipeline integration: Analyzer → AnalysisResult per dimension (v1.2.1) ──
//
// Lock-in test for run_analysis: when the input contains code that triggers
// each dimension, the corresponding AnalysisResult fields must be populated.
// Today this passes — purpose is to catch silent regressions during the
// per-dimension Finding-typing refactor (typed AnalysisFindings).

#[test]
fn test_run_analysis_populates_iosp_dimension() {
    // IOSP violation: mutual recursion ensures neither function gets safe-
    // target-reclassified back to Operation (each is the other's
    // Violation-target). Pattern matches existing iosp/tests/root.rs:test_violation_mixed.
    let code = r#"
fn helper(x: i32) { if x > 0 { violator(x); } }
fn violator(x: i32) {
    let _y = x;
    if _y > 0 {
        helper(_y);
    }
}
"#;
    let syntax = syn::parse_file(code).unwrap();
    let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
    let config = Config::default();
    let analysis = run_analysis(parsed, &config);
    assert!(
        analysis
            .results
            .iter()
            .any(|f| matches!(f.classification, Classification::Violation { .. })),
        "IOSP violation not found: {:?}",
        analysis
            .results
            .iter()
            .map(|f| (&f.name, &f.classification))
            .collect::<Vec<_>>()
    );
    assert!(analysis.summary.violations >= 1, "summary.violations == 0");
    // Typed projection: findings.iosp must mirror IOSP violations.
    assert!(
        !analysis.findings.iosp.is_empty(),
        "findings.iosp empty despite IOSP violations present"
    );
    assert!(
        analysis
            .findings
            .iosp
            .iter()
            .any(|f| f.common.rule_id == "iosp/violation"),
        "expected iosp/violation rule_id on findings.iosp"
    );
}

#[test]
fn test_run_analysis_populates_dry_duplicates() {
    // Two identical non-trivial functions should be flagged as duplicates.
    // Body is large enough to clear the default min_tokens / min_lines /
    // min_statements thresholds.
    let source = r#"
fn alpha(x: i32, y: i32) -> i32 {
    let a = x + y + 1;
    let b = a * 2;
    let c = b - 3;
    let d = c + a;
    let e = d * b;
    e + a + b + c
}
fn beta(x: i32, y: i32) -> i32 {
    let a = x + y + 1;
    let b = a * 2;
    let c = b - 3;
    let d = c + a;
    let e = d * b;
    e + a + b + c
}
"#;
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let config = Config::default();
    let analysis = run_analysis(parsed, &config);
    assert!(
        !analysis.findings.dry.is_empty(),
        "findings.dry empty — duplicate not detected"
    );
    assert!(
        analysis
            .findings
            .dry
            .iter()
            .any(|f| f.common.rule_id == "dry/duplicate/exact"),
        "expected dry/duplicate/exact rule_id on findings.dry; got {:?}",
        analysis
            .findings
            .dry
            .iter()
            .map(|f| f.common.rule_id.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_run_analysis_populates_complexity_metrics() {
    // Function with measurable complexity must have complexity metrics filled.
    let source = r#"
fn measured(x: i32) -> i32 {
    if x > 0 {
        if x > 10 {
            return x * 2;
        }
        return x + 1;
    }
    -x
}
"#;
    let syntax = syn::parse_file(source).unwrap();
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    let config = Config::default();
    let analysis = run_analysis(parsed, &config);
    let measured = analysis
        .results
        .iter()
        .find(|f| f.name == "measured")
        .expect("measured function missing from results");
    assert!(
        measured.complexity.is_some(),
        "complexity metrics missing for non-trivial function"
    );
    let m = measured.complexity.as_ref().unwrap();
    assert!(
        m.cyclomatic_complexity > 1,
        "cyclomatic_complexity should reflect branching"
    );
}

#[test]
fn test_run_analysis_findings_architecture_field_present() {
    // Architecture is configured off by default; the typed
    // `findings.architecture` field must exist and be empty.
    let parsed: Vec<(String, String, syn::File)> = vec![];
    let config = Config::default();
    let analysis = run_analysis(parsed, &config);
    assert!(analysis.findings.architecture.is_empty());
    let _len: usize = analysis.findings.architecture.len();
}

#[test]
fn test_analyze_and_output_returns_analyzed_functions() {
    // `analyze_and_output` orchestrates collect → parse → analyze → print and
    // returns the `AnalysisResult` it printed. Asserting the returned analysis
    // contains the fixture's functions pins the orchestration against being
    // replaced by a no-op (the four delegated calls being skipped).
    let tmp = test_dir();
    fs::write(
        tmp.path().join("lib.rs"),
        "fn alpha() {}\nfn beta() { alpha(); }\n",
    )
    .unwrap();
    let mut config = Config::default();
    config.compile();
    let analysis = analyze_and_output(
        tmp.path(),
        &config,
        &crate::cli::OutputFormat::Text,
        false,
        false,
    );
    assert!(
        analysis.results.iter().any(|f| f.name == "alpha"),
        "returned analysis must carry the fixture's functions, got {:?}",
        analysis.results.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
}

#[test]
fn a_qual_api_on_a_re_exported_but_unconsumed_function_is_not_spent() {
    // The end-to-end shape of "a `use` is exposure, not consumption": a library
    // root re-exports an entry point nothing in the workspace calls, and the
    // author has marked it. That marker is doing its job. Counting the
    // `pub use` as a production call reported it as spent — the one advice
    // that makes an author delete a marker holding back a real finding.
    let lib = "pub mod inner;\npub use inner::entry;";
    let inner = "// qual:api\npub fn entry() {}";
    let parsed = vec![
        (
            "src/lib.rs".to_string(),
            lib.to_string(),
            syn::parse_file(lib).unwrap(),
        ),
        (
            "src/inner.rs".to_string(),
            inner.to_string(),
            syn::parse_file(inner).unwrap(),
        ),
    ];
    let analysis = run_analysis(parsed, &Config::default());
    let orphans = &analysis.findings.orphan_suppressions;
    assert!(orphans.is_empty(), "marker still does its job: {orphans:?}");
    assert!(
        analysis
            .findings
            .dry
            .iter()
            .all(|f| !f.common.message.contains("entry")),
        "and the marker keeps the finding away: {:?}",
        analysis.findings.dry
    );
}
