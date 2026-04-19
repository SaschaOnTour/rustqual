// qual:allow(coupling) reason: "orchestrator module — high instability is expected"
pub(crate) mod discovery;
pub(crate) mod dry_suppressions;
mod metrics;
mod structural_metrics;
mod tq_metrics;
pub(crate) mod warnings;

pub(crate) use discovery::{
    collect_filtered_files, collect_rust_files, collect_suppression_lines, filter_to_changed,
    get_git_changed_files, read_and_parse_files,
};

use std::path::Path;

use crate::adapters::analyzers::iosp::scope::ProjectScope;
use crate::adapters::analyzers::iosp::{Analyzer, FunctionAnalysis};
use crate::config::Config;
use crate::report::{AnalysisResult, Summary};

use metrics::{
    apply_parameter_warnings, build_file_call_graph, compute_coupling, compute_srp,
    count_coupling_warnings, count_dry_findings, count_srp_warnings, mark_coupling_suppressions,
    mark_sdp_suppressions, mark_srp_suppressions, run_dry_detection, run_guarded_detection,
};
use structural_metrics::{
    compute_structural, count_structural_warnings, mark_structural_suppressions,
};
use tq_metrics::{compute_tq, count_tq_warnings, mark_tq_suppressions};
use warnings::{
    apply_complexity_warnings, apply_extended_warnings, apply_file_suppressions,
    check_suppression_ratio, count_all_suppressions, exclude_test_violations,
};

/// Run analysis and apply suppressions, returning all analysis results.
/// Integration: orchestrates scope building, suppression collection,
/// IOSP analysis, complexity, and delegates to sub-integrations.
pub(crate) fn run_analysis(
    parsed: &[(String, String, syn::File)],
    config: &Config,
) -> AnalysisResult {
    let scope_refs: Vec<(&str, &syn::File)> = parsed
        .iter()
        .map(|(path, _, file)| (path.as_str(), file))
        .collect();
    let scope = ProjectScope::from_files(&scope_refs);
    let suppression_lines = collect_suppression_lines(parsed);
    let analyzer = Analyzer::new(config, &scope);

    let mut all_results: Vec<_> = parsed
        .iter()
        .flat_map(|(path, _, syntax)| {
            let file_suppressions = suppression_lines.get(path);
            analyzer
                .analyze_file(syntax, path)
                .into_iter()
                .map(move |mut fa| {
                    if let Some(suppressions) = file_suppressions {
                        apply_file_suppressions(&mut fa, suppressions);
                    }
                    fa
                })
        })
        .collect();

    exclude_test_violations(&mut all_results);
    let recursive_lines = discovery::collect_recursive_lines(parsed);
    warnings::apply_recursive_annotations(&mut all_results, &recursive_lines);
    warnings::apply_leaf_reclassification(&mut all_results);
    let mut summary = Summary::from_results(&all_results);
    apply_complexity_warnings(&mut all_results, config, &mut summary);
    let unsafe_allow_lines = discovery::collect_unsafe_allow_lines(parsed);
    apply_extended_warnings(&mut all_results, config, &mut summary, &unsafe_allow_lines);

    let secondary = run_secondary_analysis(
        parsed,
        config,
        &all_results,
        &suppression_lines,
        &mut summary,
    );

    finalize_summary(&mut summary, config, &suppression_lines, parsed);

    AnalysisResult {
        results: all_results,
        summary,
        coupling: secondary.coupling,
        duplicates: secondary.duplicates,
        dead_code: secondary.dead_code,
        fragments: secondary.fragments,
        boilerplate: secondary.boilerplate,
        wildcard_warnings: secondary.wildcard_warnings,
        repeated_matches: secondary.repeated_matches,
        srp: secondary.srp,
        tq: secondary.tq,
        structural: secondary.structural,
    }
}

/// Compute quality score and suppression ratio for the final summary.
/// Operation: arithmetic + threshold checks on summary fields.
fn finalize_summary(
    summary: &mut Summary,
    config: &Config,
    suppression_lines: &std::collections::HashMap<String, Vec<crate::findings::Suppression>>,
    parsed: &[(String, String, syn::File)],
) {
    summary.compute_quality_score(&config.weights.as_array());
    summary.all_suppressions = count_all_suppressions(suppression_lines, parsed);
    summary.suppression_ratio_exceeded = check_suppression_ratio(
        summary.total,
        summary.all_suppressions,
        config.max_suppression_ratio,
    );
}

/// Results from coupling, DRY, SRP, and TQ analysis passes.
struct SecondaryResults {
    coupling: Option<crate::adapters::analyzers::coupling::CouplingAnalysis>,
    duplicates: Vec<crate::adapters::analyzers::dry::functions::DuplicateGroup>,
    dead_code: Vec<crate::adapters::analyzers::dry::dead_code::DeadCodeWarning>,
    fragments: Vec<crate::adapters::analyzers::dry::fragments::FragmentGroup>,
    boilerplate: Vec<crate::adapters::analyzers::dry::boilerplate::BoilerplateFind>,
    wildcard_warnings: Vec<crate::adapters::analyzers::dry::wildcards::WildcardImportWarning>,
    repeated_matches: Vec<crate::adapters::analyzers::dry::match_patterns::RepeatedMatchGroup>,
    srp: Option<crate::adapters::analyzers::srp::SrpAnalysis>,
    tq: Option<crate::adapters::analyzers::tq::TqAnalysis>,
    structural: Option<crate::adapters::analyzers::structural::StructuralAnalysis>,
}

/// Run coupling, DRY, SRP, and TQ analysis passes, updating summary counts.
/// Integration: orchestrates detection sub-functions, no logic.
fn run_secondary_analysis(
    parsed: &[(String, String, syn::File)],
    config: &Config,
    all_results: &[FunctionAnalysis],
    suppression_lines: &std::collections::HashMap<String, Vec<crate::findings::Suppression>>,
    summary: &mut Summary,
) -> SecondaryResults {
    let api_lines = discovery::collect_api_lines(parsed);

    let mut coupling = compute_coupling(parsed, config);
    mark_coupling_suppressions(coupling.as_mut(), suppression_lines);
    mark_sdp_suppressions(coupling.as_mut());
    count_coupling_warnings(coupling.as_mut(), &config.coupling, summary);

    let mut dry = run_dry_detection(parsed, config, suppression_lines, &api_lines, summary);
    dry_suppressions::mark_dry_suppressions(&mut dry.duplicates, suppression_lines);
    let inverse_lines = discovery::collect_inverse_lines(parsed);
    dry_suppressions::mark_inverse_suppressions(&mut dry.duplicates, &inverse_lines);
    dry_suppressions::mark_dry_suppressions(&mut dry.fragments, suppression_lines);
    dry_suppressions::mark_dry_suppressions(&mut dry.boilerplate, suppression_lines);
    use crate::adapters::analyzers::dry::match_patterns::detect_repeated_matches;
    let mut repeated_matches = run_guarded_detection(
        config.duplicates.detect_repeated_matches,
        |p, c| detect_repeated_matches(p, &c.duplicates),
        parsed,
        config,
    );
    dry_suppressions::mark_dry_suppressions(&mut repeated_matches, suppression_lines);
    count_dry_findings(&dry, &repeated_matches, summary);

    metrics::count_sdp_violations(coupling.as_ref(), &config.coupling, summary);

    let file_call_graph = build_file_call_graph(all_results);
    let mut srp = compute_srp(parsed, config, &file_call_graph);
    apply_parameter_warnings(all_results, srp.as_mut(), &config.srp);
    mark_srp_suppressions(srp.as_mut(), suppression_lines);
    count_srp_warnings(srp.as_ref(), summary);

    let scope_refs: Vec<(&str, &syn::File)> = parsed
        .iter()
        .map(|(path, _, file)| (path.as_str(), file))
        .collect();
    let tq_scope = ProjectScope::from_files(&scope_refs);
    let mut tq = compute_tq(parsed, config, &tq_scope, all_results, &dry.dead_code);
    mark_tq_suppressions(tq.as_mut(), suppression_lines);
    count_tq_warnings(tq.as_ref(), summary);

    let mut structural = compute_structural(parsed, config);
    mark_structural_suppressions(structural.as_mut(), suppression_lines);
    count_structural_warnings(structural.as_ref(), summary);

    SecondaryResults {
        coupling,
        duplicates: dry.duplicates,
        dead_code: dry.dead_code,
        fragments: dry.fragments,
        boilerplate: dry.boilerplate,
        wildcard_warnings: dry.wildcard_warnings,
        repeated_matches,
        srp,
        tq,
        structural,
    }
}

/// Run a full analysis pipeline on a set of files and produce output.
/// Integration: orchestrates read_and_parse_files, run_analysis, output_results.
pub(crate) fn analyze_and_output(
    path: &Path,
    config: &Config,
    output_format: &super::OutputFormat,
    verbose: bool,
    suggestions: bool,
) {
    let files = collect_filtered_files(path, config);
    let parsed = read_and_parse_files(&files, path);
    let analysis = run_analysis(&parsed, config);
    output_results(&analysis, output_format, verbose, suggestions, config);
}

/// Output results in the requested format.
/// Operation: match on output format.
pub(crate) fn output_results(
    analysis: &AnalysisResult,
    output_format: &super::OutputFormat,
    verbose: bool,
    suggestions: bool,
    config: &crate::config::Config,
) {
    use crate::report;
    match output_format {
        super::OutputFormat::Json => report::print_json(analysis),
        super::OutputFormat::Github => {
            report::print_github(&analysis.results, &analysis.summary);
            analysis
                .coupling
                .iter()
                .for_each(|ca| report::print_coupling_annotations(ca, &config.coupling));
            report::print_dry_annotations(analysis);
            analysis.srp.iter().for_each(report::print_srp_annotations);
            analysis.tq.iter().for_each(report::print_tq_annotations);
            analysis
                .structural
                .iter()
                .for_each(report::print_structural_annotations);
        }
        super::OutputFormat::Dot => report::print_dot(&analysis.results),
        super::OutputFormat::Sarif => report::print_sarif(analysis),
        super::OutputFormat::Html => report::print_html(analysis),
        super::OutputFormat::Ai => report::print_ai(analysis, config),
        super::OutputFormat::AiJson => report::print_ai_json(analysis, config),
        super::OutputFormat::Text => {
            let findings = crate::report::findings_list::collect_all_findings(analysis);
            // Summary first — always shown
            report::print_summary_only(&analysis.summary, &findings);
            // Coupling table — always shown (explanation text only with --verbose)
            analysis
                .coupling
                .iter()
                .for_each(|ca| report::print_coupling_section(ca, &config.coupling, verbose));
            if verbose {
                // Verbose: file-grouped output + detail sections (summary already printed above)
                report::print_files_only(&analysis.results);
                report::print_dry_section(analysis);
                analysis.srp.iter().for_each(report::print_srp_section);
                analysis.tq.iter().for_each(report::print_tq_section);
                analysis
                    .structural
                    .iter()
                    .for_each(report::print_structural_section);
            } else {
                // Default: compact findings list
                crate::report::findings_list::print_findings(&findings);
            }
            if suggestions {
                report::print_suggestions(&analysis.results);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::analyzers::iosp::Classification;
    use crate::config::Config;
    use crate::findings::Suppression;
    use std::fs;

    use super::discovery::collect_rust_files;
    use metrics::{count_coupling_warnings, mark_coupling_suppressions};
    use warnings::{check_suppression_ratio, count_all_suppressions};

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

    #[test]
    fn test_collect_rust_files_skips_target() {
        let tmp = test_dir();
        let target_dir = tmp.path().join("target");
        fs::create_dir(&target_dir).unwrap();
        fs::write(target_dir.join("compiled.rs"), "fn c() {}").unwrap();
        fs::write(tmp.path().join("src.rs"), "fn s() {}").unwrap();
        let files = collect_rust_files(tmp.path());
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_collect_rust_files_skips_hidden() {
        let tmp = test_dir();
        let hidden_dir = tmp.path().join(".hidden");
        fs::create_dir(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("secret.rs"), "fn h() {}").unwrap();
        fs::write(tmp.path().join("visible.rs"), "fn v() {}").unwrap();
        let files = collect_rust_files(tmp.path());
        assert_eq!(files.len(), 1);
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
    fn test_collect_suppression_qual_allow_all() {
        let source = "// qual:allow\nfn foo() {}";
        let syntax = syn::parse_file(source).unwrap();
        let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
        let result = collect_suppression_lines(&parsed);
        assert!(result.contains_key("test.rs"));
        let sups = &result["test.rs"];
        assert_eq!(sups.len(), 1);
        assert!(sups[0].dimensions.is_empty()); // all dimensions
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
        let source = "// qual:allow\nfn foo() {}\n// qual:allow(iosp)\nfn bar() {}";
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
        let analysis = run_analysis(&parsed, &config);
        assert!(analysis.results.is_empty());
        assert_eq!(analysis.summary.total, 0);
    }

    #[test]
    fn test_run_analysis_trivial_function() {
        let source = "fn empty() {}";
        let syntax = syn::parse_file(source).unwrap();
        let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
        let config = Config::default();
        let analysis = run_analysis(&parsed, &config);
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
    fn test_output_results_text_no_panic() {
        let results = vec![];
        let summary = crate::report::Summary::from_results(&results);
        let analysis = AnalysisResult {
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
        }
    }

    #[test]
    fn test_mark_coupling_suppressions_marks_module() {
        let mut analysis = make_coupling_analysis();
        let sup = Suppression {
            line: 1,
            dimensions: vec![crate::findings::Dimension::Coupling],
            reason: Some("orchestrator module".to_string()),
        };
        let mut suppression_lines = std::collections::HashMap::new();
        suppression_lines.insert("pipeline.rs".to_string(), vec![sup]);

        mark_coupling_suppressions(Some(&mut analysis), &suppression_lines);

        assert!(analysis.metrics[0].suppressed); // pipeline
        assert!(!analysis.metrics[1].suppressed); // config
    }

    #[test]
    fn test_mark_coupling_suppressions_qual_allow_all_covers_coupling() {
        let mut analysis = make_coupling_analysis();
        let sup = Suppression {
            line: 1,
            dimensions: vec![], // all dimensions
            reason: None,
        };
        let mut suppression_lines = std::collections::HashMap::new();
        suppression_lines.insert("pipeline.rs".to_string(), vec![sup]);

        mark_coupling_suppressions(Some(&mut analysis), &suppression_lines);

        assert!(analysis.metrics[0].suppressed); // pipeline
    }

    #[test]
    fn test_mark_coupling_suppressions_iosp_only_does_not_cover_coupling() {
        let mut analysis = make_coupling_analysis();
        let sup = Suppression {
            line: 1,
            dimensions: vec![crate::findings::Dimension::Iosp],
            reason: None,
        };
        let mut suppression_lines = std::collections::HashMap::new();
        suppression_lines.insert("pipeline.rs".to_string(), vec![sup]);

        mark_coupling_suppressions(Some(&mut analysis), &suppression_lines);

        assert!(!analysis.metrics[0].suppressed); // not suppressed
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
        };
        let sup = Suppression {
            line: 1,
            dimensions: vec![crate::findings::Dimension::Coupling],
            reason: None,
        };
        let mut suppression_lines = std::collections::HashMap::new();
        // Suppression in a submodule file maps to the top-level module
        suppression_lines.insert("analyzer/visitor.rs".to_string(), vec![sup]);

        mark_coupling_suppressions(Some(&mut analysis), &suppression_lines);

        assert!(analysis.metrics[0].suppressed); // analyzer suppressed
    }

    #[test]
    fn test_mark_coupling_suppressions_none_analysis() {
        let suppression_lines = std::collections::HashMap::new();
        // Should not panic
        mark_coupling_suppressions(None, &suppression_lines);
    }

    #[test]
    fn test_count_coupling_warnings_skips_suppressed() {
        let mut analysis = make_coupling_analysis();
        analysis.metrics[0].suppressed = true; // pipeline suppressed

        let config = crate::config::sections::CouplingConfig::default();
        let mut summary = Summary::from_results(&[]);

        count_coupling_warnings(Some(&mut analysis), &config, &mut summary);

        assert_eq!(summary.coupling_warnings, 0); // pipeline warning suppressed
    }

    #[test]
    fn test_count_coupling_warnings_counts_unsuppressed() {
        let mut analysis = make_coupling_analysis();

        let config = crate::config::sections::CouplingConfig::default();
        let mut summary = Summary::from_results(&[]);

        count_coupling_warnings(Some(&mut analysis), &config, &mut summary);

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
        };

        let config = crate::config::sections::CouplingConfig::default();
        let mut summary = Summary::from_results(&[]);

        count_coupling_warnings(Some(&mut analysis), &config, &mut summary);

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
                },
                crate::findings::Suppression {
                    line: 3,
                    dimensions: vec![crate::findings::Dimension::Iosp],
                    reason: None,
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
        let source = "#[allow(dead_code)]\nfn unused() {}\n// qual:allow\nfn foo() {}";
        let syntax = syn::parse_file(source).unwrap();
        let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
        let mut supp = std::collections::HashMap::new();
        supp.insert(
            "test.rs".to_string(),
            vec![crate::findings::Suppression {
                line: 3,
                dimensions: vec![],
                reason: None,
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
}
