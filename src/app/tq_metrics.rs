use crate::adapters::analyzers::iosp::FunctionAnalysis;
use crate::config::Config;
use crate::findings::Suppression;
use crate::report::Summary;

/// Compute test quality analysis if enabled, plus the `qual:api` /
/// `qual:test_helper` markers that have gone stale. Both come from one pass
/// because the staleness rule needs exactly what TQ already built — the
/// marked declarations and the production call set — and recomputing them
/// would mean a second walk over every file. With TQ disabled the pass does
/// not run, so no marker is reported (silence, never a false "delete me").
/// Operation: conditional check + data collection + module-qualified call.
pub(super) fn compute_tq(
    parsed: &[(String, String, syn::File)],
    config: &Config,
    all_results: &[FunctionAnalysis],
    dead_code: &[crate::adapters::analyzers::dry::dead_code::DeadCodeWarning],
    annotation_lines: &super::metrics::AnnotationLines<'_>,
) -> (
    Option<crate::adapters::analyzers::tq::TqAnalysis>,
    Vec<crate::domain::findings::OrphanSuppression>,
) {
    if !config.test_quality.enabled {
        return (None, Vec::new());
    }
    let scope_refs: Vec<(&str, &syn::File)> = parsed
        .iter()
        .map(|(path, _, file)| (path.as_str(), file))
        .collect();
    let scope = crate::adapters::shared::project_scope::ProjectScope::from_files(&scope_refs);
    let mut declared_fns = crate::adapters::analyzers::dry::collect_declared_functions(parsed);
    crate::adapters::analyzers::dry::dead_code::mark_api_declarations(
        &mut declared_fns,
        annotation_lines.api,
    );
    crate::adapters::analyzers::dry::dead_code::mark_test_helper_declarations(
        &mut declared_fns,
        annotation_lines.test_helper,
    );
    let cfg_test_files =
        crate::adapters::analyzers::dry::dead_code::collect_cfg_test_file_paths(parsed);
    let calls =
        crate::adapters::analyzers::dry::dead_code::collect_all_calls(parsed, &cfg_test_files);
    // TQ-003 asks whether production *calls* a function, so a `pub use`
    // re-export does not qualify it — the marker check and DRY-002 do want the
    // re-export, since there the question is whether anything uses it at all.
    let prod_calls = calls.production;
    let test_calls = calls.tests;
    let mut used_in_production = prod_calls.clone();
    used_in_production.extend(calls.reexported);
    let coverage_path = config
        .test_quality
        .coverage_file
        .as_ref()
        .map(std::path::Path::new);
    let ctx = crate::adapters::analyzers::tq::TqContext {
        parsed,
        scope: &scope,
        config,
        declared_fns: &declared_fns,
        prod_calls: &prod_calls,
        test_calls: &test_calls,
        all_results,
        dead_code,
        coverage_path,
    };
    let analysis = crate::adapters::analyzers::tq::analyze_test_quality(&ctx);
    let stale = detect_stale_markers(parsed, annotation_lines, &declared_fns, &used_in_production);
    (Some(analysis), stale)
}

/// The `qual:api` / `qual:test_helper` markers that have gone stale.
/// Integration: type collection + marking + the check itself.
fn detect_stale_markers(
    parsed: &[(String, String, syn::File)],
    annotation_lines: &super::metrics::AnnotationLines<'_>,
    declared_fns: &[crate::adapters::shared::declared_function::DeclaredFunction],
    prod_calls: &std::collections::HashSet<String>,
) -> Vec<crate::domain::findings::OrphanSuppression> {
    use crate::adapters::shared::marked_declaration::mark_annotated;
    let cfg_test_files =
        crate::adapters::analyzers::dry::dead_code::collect_cfg_test_file_paths(parsed);
    let mut declared_types =
        crate::adapters::analyzers::dry::collect_declared_types(parsed, &cfg_test_files);
    mark_annotated(&mut declared_types, annotation_lines.api, |d| {
        d.is_api = true
    });
    mark_annotated(&mut declared_types, annotation_lines.test_helper, |d| {
        d.is_test_helper = true
    });
    let (prod_refs, _) =
        crate::adapters::analyzers::dry::collect_type_references(parsed, &cfg_test_files);
    let reach = crate::adapters::shared::reachability::compute_external_reach(parsed);
    crate::app::stale_markers::detect_stale_marker_orphans(
        &crate::app::stale_markers::MarkerContext {
            declared_fns,
            declared_types: &declared_types,
            prod_calls,
            prod_refs: &prod_refs,
            api_lines: annotation_lines.api,
            test_helper_lines: annotation_lines.test_helper,
            reach: &reach,
        },
    )
}

/// Mark TQ warnings as suppressed based on `// qual:allow(test_quality[, kind])`
/// comments. A blanket allow silences every TQ kind; a targeted form silences
/// one (`no_assertion` / `no_sut` / `untested` / `uncovered` / `untested_logic`).
/// Operation: iteration + per-kind suppression check via closures.
pub(super) fn mark_tq_suppressions(
    tq: Option<&mut crate::adapters::analyzers::tq::TqAnalysis>,
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
) {
    let Some(tq) = tq else { return };
    let tq_dim = crate::domain::Dimension::TestQuality;
    // Window width shared with the orphan detector, see
    // `app::suppression_windows::TQ`.
    let window = super::suppression_windows::TQ;
    tq.warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            let target = tq_target_name(&w.kind);
            w.suppressed = sups.iter().any(|sup| {
                let in_window = sup.line <= w.line && w.line - sup.line <= window;
                in_window && sup.suppresses(tq_dim, target, None)
            });
        }
    });
}

/// The `allow(test_quality, <target>)` name for a TQ warning kind.
/// Operation: enum dispatch, no own calls.
fn tq_target_name(kind: &crate::adapters::analyzers::tq::TqWarningKind) -> &'static str {
    use crate::adapters::analyzers::tq::TqWarningKind as K;
    match kind {
        K::NoAssertion => "no_assertion",
        K::NoSut => "no_sut",
        K::Untested => "untested",
        K::Uncovered => "uncovered",
        K::UntestedLogic { .. } => "untested_logic",
    }
}

/// Count TQ warnings and update summary, excluding suppressed entries.
/// Operation: iteration + conditional counting, no own calls.
pub(super) fn count_tq_warnings(
    tq: Option<&crate::adapters::analyzers::tq::TqAnalysis>,
    summary: &mut Summary,
) {
    let Some(tq) = tq else { return };
    tq.warnings
        .iter()
        .filter(|w| !w.suppressed)
        .for_each(|w| match &w.kind {
            crate::adapters::analyzers::tq::TqWarningKind::NoAssertion => {
                summary.tq_no_assertion_warnings += 1
            }
            crate::adapters::analyzers::tq::TqWarningKind::NoSut => summary.tq_no_sut_warnings += 1,
            crate::adapters::analyzers::tq::TqWarningKind::Untested => {
                summary.tq_untested_warnings += 1
            }
            crate::adapters::analyzers::tq::TqWarningKind::Uncovered => {
                summary.tq_uncovered_warnings += 1
            }
            crate::adapters::analyzers::tq::TqWarningKind::UntestedLogic { .. } => {
                summary.tq_untested_logic_warnings += 1;
            }
        });
}
