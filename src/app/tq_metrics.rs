use crate::adapters::analyzers::iosp::FunctionAnalysis;
use crate::config::Config;
use crate::findings::Suppression;
use crate::report::Summary;

/// Compute test quality analysis if enabled.
/// Operation: conditional check + data collection + module-qualified call.
pub(super) fn compute_tq(
    parsed: &[(String, String, syn::File)],
    config: &Config,
    all_results: &[FunctionAnalysis],
    dead_code: &[crate::adapters::analyzers::dry::dead_code::DeadCodeWarning],
    annotation_lines: &super::metrics::AnnotationLines<'_>,
) -> Option<crate::adapters::analyzers::tq::TqAnalysis> {
    if !config.test_quality.enabled {
        return None;
    }
    let scope_refs: Vec<(&str, &syn::File)> = parsed
        .iter()
        .map(|(path, _, file)| (path.as_str(), file))
        .collect();
    let scope = crate::adapters::analyzers::iosp::scope::ProjectScope::from_files(&scope_refs);
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
    let (prod_calls, test_calls) =
        crate::adapters::analyzers::dry::dead_code::collect_all_calls(parsed, &cfg_test_files);
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
    Some(crate::adapters::analyzers::tq::analyze_test_quality(&ctx))
}

/// Mark TQ warnings as suppressed based on `// qual:allow(test)` comments.
/// Operation: iteration + suppression check, no own calls.
pub(super) fn mark_tq_suppressions(
    tq: Option<&mut crate::adapters::analyzers::tq::TqAnalysis>,
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
) {
    let Some(tq) = tq else { return };
    let tq_dim = crate::domain::Dimension::TestQuality;
    tq.warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            w.suppressed = sups.iter().any(|sup| {
                let in_window = sup.line <= w.line && w.line - sup.line <= 5;
                in_window && sup.covers(tq_dim)
            });
        }
    });
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
