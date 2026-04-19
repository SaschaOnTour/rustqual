// qual:allow(coupling) reason: "orchestrator module — high instability is expected"
mod architecture;
pub(crate) mod dry_suppressions;
mod metrics;
mod structural_metrics;
mod tq_metrics;
pub(crate) mod warnings;

use architecture::collect_architecture_findings;

pub(crate) use crate::adapters::source::filesystem as discovery;
pub(crate) use crate::adapters::source::filesystem::{
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

/// Bundle returned by the primary IOSP + complexity pass.
struct PrimaryResults {
    all_results: Vec<FunctionAnalysis>,
    summary: Summary,
    suppression_lines: std::collections::HashMap<String, Vec<crate::findings::Suppression>>,
}

/// Run the primary (IOSP + complexity) analysis pass.
/// Integration: delegates scope build, per-file analyze, reclassification, warnings.
fn run_primary_analysis(parsed: &[(String, String, syn::File)], config: &Config) -> PrimaryResults {
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
    let PrimaryResults {
        mut all_results,
        mut summary,
        suppression_lines,
    } = run_primary_analysis(parsed, config);
    let secondary = run_secondary_analysis(
        parsed,
        config,
        &all_results,
        &suppression_lines,
        &mut summary,
    );
    let architecture_findings =
        collect_architecture_findings(parsed, config, &suppression_lines, &mut summary);
    finalize_summary(&mut summary, config, &suppression_lines, parsed);
    build_result(&mut all_results, summary, secondary, architecture_findings)
}

/// Assemble the final AnalysisResult. The `_results` parameter is &mut to
/// allow callers to mutate before the move; the body moves it in.
/// Operation: struct construction, no own calls.
fn build_result(
    all_results: &mut Vec<FunctionAnalysis>,
    summary: Summary,
    secondary: SecondaryResults,
    architecture_findings: Vec<crate::domain::Finding>,
) -> AnalysisResult {
    AnalysisResult {
        results: std::mem::take(all_results),
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
        architecture_findings,
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
mod tests;
