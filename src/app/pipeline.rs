// qual:allow(coupling) reason: "orchestrator module — high instability is expected"
use super::architecture::collect_architecture_findings;
use super::secondary::{run_secondary_analysis, SecondaryContext, SecondaryResults};
use super::warnings;

use crate::adapters::source::filesystem as discovery;
use crate::adapters::source::filesystem::{
    collect_filtered_files, collect_suppression_lines, read_and_parse_files,
};

use std::path::Path;

use crate::adapters::analyzers::iosp::scope::ProjectScope;
use crate::adapters::analyzers::iosp::{Analyzer, FunctionAnalysis};
use crate::config::Config;
use crate::report::{AnalysisResult, Summary};

use super::warnings::{
    apply_complexity_warnings, apply_extended_warnings, apply_file_suppressions,
    check_suppression_ratio, count_all_suppressions, exclude_test_violations,
};

/// Bundle returned by the primary IOSP + complexity pass.
struct PrimaryResults {
    all_results: Vec<FunctionAnalysis>,
    summary: Summary,
    suppression_lines: std::collections::HashMap<String, Vec<crate::findings::Suppression>>,
}

/// Run the primary (IOSP + complexity) analysis pass.
/// Integration: delegates scope build, per-file analyze, reclassification, warnings.
fn run_primary_analysis(
    parsed: &[(String, String, syn::File)],
    config: &Config,
    cfg_test_files: &std::collections::HashSet<String>,
) -> PrimaryResults {
    let scope_refs: Vec<(&str, &syn::File)> = parsed
        .iter()
        .map(|(path, _, file)| (path.as_str(), file))
        .collect();
    let scope = ProjectScope::from_files(&scope_refs);
    let suppression_lines = collect_suppression_lines(parsed);
    let analyzer = Analyzer::new(config, &scope).with_cfg_test_files(cfg_test_files);
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
    PrimaryResults {
        all_results,
        summary,
        suppression_lines,
    }
}

/// Run analysis and apply suppressions, returning all analysis results.
/// Integration: orchestrates primary + secondary + architecture passes.
pub(crate) fn run_analysis(
    parsed: &[(String, String, syn::File)],
    config: &Config,
) -> AnalysisResult {
    // Compute once and thread through both passes — IOSP uses it to mark
    // in-cfg-test functions as test code; DRY dead-code uses it to split
    // prod vs test calls. Previously built twice.
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(parsed);
    let PrimaryResults {
        mut all_results,
        mut summary,
        suppression_lines,
    } = run_primary_analysis(parsed, config, &cfg_test_files);
    let secondary_ctx = SecondaryContext {
        parsed,
        config,
        all_results: &all_results,
        suppression_lines: &suppression_lines,
        cfg_test_files: &cfg_test_files,
    };
    let secondary = run_secondary_analysis(&secondary_ctx, &mut summary);
    let architecture_findings =
        collect_architecture_findings(parsed, config, &suppression_lines, &mut summary);
    finalize_summary(&mut summary, config, &suppression_lines, parsed);
    let mut result = build_result(
        &mut all_results,
        summary,
        secondary,
        architecture_findings,
        config,
    );
    let orphans = crate::app::orphan_suppressions::detect_orphan_suppressions(
        &suppression_lines,
        &result,
        config,
    );
    result.summary.orphan_suppressions = orphans.len();
    result.findings.orphan_suppressions = orphans;
    result
}

/// Assemble the final AnalysisResult. The `_results` parameter is &mut to
/// allow callers to mutate before the move; the body moves it in.
/// Operation: struct construction, no own calls.
fn build_result(
    all_results: &mut Vec<FunctionAnalysis>,
    summary: Summary,
    secondary: SecondaryResults,
    architecture_findings: Vec<crate::domain::Finding>,
    config: &crate::config::Config,
) -> AnalysisResult {
    let findings = crate::domain::AnalysisFindings {
        iosp: super::projection::project_iosp(all_results),
        complexity: super::projection::project_complexity(all_results, config),
        architecture: super::projection::project_architecture(&architecture_findings),
        dry: super::projection::project_dry(&secondary),
        srp: super::projection::project_srp(secondary.srp.as_ref(), secondary.structural.as_ref()),
        coupling: super::projection::project_coupling(
            secondary.coupling.as_ref(),
            secondary.structural.as_ref(),
        ),
        test_quality: super::projection::project_tq(secondary.tq.as_ref()),
        orphan_suppressions: Vec::new(),
    };
    let data = super::projection::project_data(all_results, secondary.coupling.as_ref());
    AnalysisResult {
        results: std::mem::take(all_results),
        summary,
        findings,
        data,
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

/// Run a full analysis pipeline on a set of files and produce output.
/// Integration: orchestrates read_and_parse_files, run_analysis, output_results.
pub(crate) fn analyze_and_output(
    path: &Path,
    config: &Config,
    output_format: &crate::cli::OutputFormat,
    verbose: bool,
    suggestions: bool,
) {
    let files = collect_filtered_files(path, config);
    let parsed = read_and_parse_files(&files, path);
    let analysis = run_analysis(&parsed, config);
    output_results(&analysis, output_format, verbose, suggestions, config);
}

/// Output results in the requested format.
/// Integration: dispatches on the output format. Most branches use the
/// existing `print_*` wrappers (each a thin `<Reporter>.render() +
/// print` shim). Two branches construct reporters directly:
/// - AI variants pass the `AiOutputFormat` flag.
/// - Text constructs a `TextReporter` so it can post-print suggestions
///   when `--suggestions` is set (a Phase 9.5 cleanup folds that into
///   the reporter once `print_suggestions` migrates to a `format_*`
///   helper).
pub(crate) fn output_results(
    analysis: &AnalysisResult,
    output_format: &crate::cli::OutputFormat,
    verbose: bool,
    suggestions: bool,
    config: &crate::config::Config,
) {
    use crate::report;
    use crate::report::findings_list::collect_all_findings;
    match output_format {
        crate::cli::OutputFormat::Json => report::print_json(analysis),
        crate::cli::OutputFormat::Github => report::print_github(analysis),
        crate::cli::OutputFormat::Dot => report::print_dot(&analysis.data),
        crate::cli::OutputFormat::Sarif => report::print_sarif(analysis),
        crate::cli::OutputFormat::Html => report::print_html(analysis),
        crate::cli::OutputFormat::Ai => report::print_ai(analysis, config),
        crate::cli::OutputFormat::AiJson => report::print_ai_json(analysis, config),
        crate::cli::OutputFormat::Text => {
            let findings_entries = collect_all_findings(analysis);
            crate::report::text::print_text(analysis, &findings_entries, verbose, None);
            if suggestions {
                report::print_suggestions(&analysis.results);
            }
        }
    }
}
