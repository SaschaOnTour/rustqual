//! Secondary analysis pass — everything after the primary IOSP and
//! complexity sweep: coupling, DRY, SRP, test quality, and structural
//! checks. Each per-dimension pass is a small helper that takes a shared
//! context plus the growing summary. The orchestrator
//! (`run_secondary_analysis`) walks the passes in dependency order and
//! gathers their outputs into one `SecondaryResults` bundle.

use crate::adapters::analyzers::iosp::FunctionAnalysis;
use crate::config::Config;
use crate::report::Summary;

use super::dry_suppressions;
use super::metrics::{
    self, apply_parameter_warnings, build_file_call_graph, compute_coupling, compute_srp,
    count_coupling_warnings, count_dry_findings, count_srp_warnings, mark_coupling_suppressions,
    mark_srp_suppressions, run_dry_detection, run_guarded_detection,
};
use super::structural_metrics::{
    compute_structural, count_structural_warnings, mark_structural_suppressions,
};
use super::tq_metrics::{compute_tq, count_tq_warnings, mark_tq_suppressions};
use crate::adapters::source::filesystem as discovery;

/// Results from coupling, DRY, SRP, TQ, and structural analysis passes.
pub(super) struct SecondaryResults {
    pub(super) coupling: Option<crate::adapters::analyzers::coupling::CouplingAnalysis>,
    pub(super) duplicates: Vec<crate::adapters::analyzers::dry::functions::DuplicateGroup>,
    pub(super) dead_code: Vec<crate::adapters::analyzers::dry::dead_code::DeadCodeWarning>,
    pub(super) fragments: Vec<crate::adapters::analyzers::dry::fragments::FragmentGroup>,
    pub(super) boilerplate: Vec<crate::adapters::analyzers::dry::boilerplate::BoilerplateFind>,
    pub(super) wildcard_warnings:
        Vec<crate::adapters::analyzers::dry::wildcards::WildcardImportWarning>,
    pub(super) repeated_matches:
        Vec<crate::adapters::analyzers::dry::match_patterns::RepeatedMatchGroup>,
    pub(super) srp: Option<crate::adapters::analyzers::srp::SrpAnalysis>,
    pub(super) tq: Option<crate::adapters::analyzers::tq::TqAnalysis>,
    pub(super) structural: Option<crate::adapters::analyzers::structural::StructuralAnalysis>,
}

/// Inputs the secondary passes share: parsed workspace, config,
/// pre-computed suppression + cfg-test indexes, and the primary pass's
/// IOSP results. Bundled to keep per-pass signatures narrow.
pub(super) struct SecondaryContext<'a> {
    pub(super) parsed: &'a [(String, String, syn::File)],
    pub(super) config: &'a Config,
    pub(super) all_results: &'a [FunctionAnalysis],
    pub(super) suppression_lines:
        &'a std::collections::HashMap<String, Vec<crate::findings::Suppression>>,
    pub(super) cfg_test_files: &'a std::collections::HashSet<String>,
}

/// Run coupling, DRY, SRP, and TQ analysis passes, updating summary counts.
/// Integration: orchestrates detection sub-functions, no logic.
pub(super) fn run_secondary_analysis(
    ctx: &SecondaryContext<'_>,
    summary: &mut Summary,
) -> SecondaryResults {
    let api_lines = discovery::collect_api_lines(ctx.parsed);
    let test_helper_lines = discovery::collect_test_helper_lines(ctx.parsed);
    let annotation_lines = metrics::AnnotationLines {
        api: &api_lines,
        test_helper: &test_helper_lines,
    };

    let coupling = run_coupling_pass(ctx, summary);
    let (dry, repeated_matches) = run_dry_pass(ctx, &annotation_lines, summary);
    metrics::count_sdp_violations(coupling.as_ref(), &ctx.config.coupling, summary);

    let srp = run_srp_pass(ctx, summary);
    let tq = run_tq_pass(ctx, &annotation_lines, &dry.dead_code, summary);
    let structural = run_structural_pass(ctx, summary);

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

/// Run coupling analysis + suppressions + warning count.
/// Integration: delegates compute + suppression + count.
fn run_coupling_pass(
    ctx: &SecondaryContext<'_>,
    summary: &mut Summary,
) -> Option<crate::adapters::analyzers::coupling::CouplingAnalysis> {
    let mut coupling = compute_coupling(ctx.parsed, ctx.config);
    mark_coupling_suppressions(coupling.as_mut(), ctx.suppression_lines);
    // Populate SDP violations AFTER suppressions are marked on metrics
    // so each violation inherits the correct `suppressed` state at
    // creation time (no separate mark pass needed).
    if let Some(ca) = coupling.as_mut() {
        crate::adapters::analyzers::coupling::populate_sdp_violations(ca);
    }
    count_coupling_warnings(coupling.as_mut(), &ctx.config.coupling, summary);
    coupling
}

/// Run DRY detection + inverse/exact/fragment/boilerplate/repeated-match suppressions.
/// Integration: delegates detection + per-category suppressions + count.
fn run_dry_pass(
    ctx: &SecondaryContext<'_>,
    annotation_lines: &metrics::AnnotationLines<'_>,
    summary: &mut Summary,
) -> (
    metrics::DryResults,
    Vec<crate::adapters::analyzers::dry::match_patterns::RepeatedMatchGroup>,
) {
    let mut dry = run_dry_detection(
        ctx.parsed,
        ctx.config,
        ctx.suppression_lines,
        annotation_lines,
        ctx.cfg_test_files,
    );
    dry_suppressions::mark_dry_suppressions(&mut dry.duplicates, ctx.suppression_lines);
    let inverse_lines = discovery::collect_inverse_lines(ctx.parsed);
    dry_suppressions::mark_inverse_suppressions(&mut dry.duplicates, &inverse_lines);
    dry_suppressions::mark_dry_suppressions(&mut dry.fragments, ctx.suppression_lines);
    dry_suppressions::mark_dry_suppressions(&mut dry.boilerplate, ctx.suppression_lines);
    use crate::adapters::analyzers::dry::match_patterns::detect_repeated_matches;
    let mut repeated_matches = run_guarded_detection(
        ctx.config.duplicates.detect_repeated_matches,
        |p, c| detect_repeated_matches(p, &c.duplicates),
        ctx.parsed,
        ctx.config,
    );
    dry_suppressions::mark_dry_suppressions(&mut repeated_matches, ctx.suppression_lines);
    count_dry_findings(&dry, &repeated_matches, summary);
    (dry, repeated_matches)
}

/// Run SRP analysis + parameter/struct suppressions + count.
/// Integration: delegates compute + parameter warnings + suppression + count.
fn run_srp_pass(
    ctx: &SecondaryContext<'_>,
    summary: &mut Summary,
) -> Option<crate::adapters::analyzers::srp::SrpAnalysis> {
    let file_call_graph = build_file_call_graph(ctx.all_results);
    let mut srp = compute_srp(ctx.parsed, ctx.config, &file_call_graph);
    apply_parameter_warnings(ctx.all_results, srp.as_mut(), &ctx.config.srp);
    mark_srp_suppressions(srp.as_mut(), ctx.suppression_lines);
    count_srp_warnings(srp.as_ref(), summary);
    srp
}

/// Run Test-Quality analysis + suppressions + count.
/// `annotation_lines` is threaded in from the shared collection above
/// so `compute_tq` doesn't re-scan every source file for the `qual:api`
/// and `qual:test_helper` markers that DRY already collected.
/// Integration: delegates scope build + compute + suppression + count.
fn run_tq_pass(
    ctx: &SecondaryContext<'_>,
    annotation_lines: &metrics::AnnotationLines<'_>,
    dead_code: &[crate::adapters::analyzers::dry::dead_code::DeadCodeWarning],
    summary: &mut Summary,
) -> Option<crate::adapters::analyzers::tq::TqAnalysis> {
    let mut tq = compute_tq(
        ctx.parsed,
        ctx.config,
        ctx.all_results,
        dead_code,
        annotation_lines,
    );
    mark_tq_suppressions(tq.as_mut(), ctx.suppression_lines);
    count_tq_warnings(tq.as_ref(), summary);
    tq
}

/// Run structural-binary checks + suppressions + count.
/// Integration: delegates compute + suppression + count.
fn run_structural_pass(
    ctx: &SecondaryContext<'_>,
    summary: &mut Summary,
) -> Option<crate::adapters::analyzers::structural::StructuralAnalysis> {
    let mut structural = compute_structural(ctx.parsed, ctx.config);
    mark_structural_suppressions(structural.as_mut(), ctx.suppression_lines);
    count_structural_warnings(structural.as_ref(), summary);
    structural
}
