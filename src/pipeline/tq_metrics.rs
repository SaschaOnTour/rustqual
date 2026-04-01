use crate::analyzer::FunctionAnalysis;
use crate::config::Config;
use crate::findings::Suppression;
use crate::report::Summary;

/// Compute test quality analysis if enabled.
/// Operation: conditional check + data collection + module-qualified call.
pub(super) fn compute_tq(
    parsed: &[(String, String, syn::File)],
    config: &Config,
    scope: &crate::scope::ProjectScope,
    all_results: &[FunctionAnalysis],
    dead_code: &[crate::dry::dead_code::DeadCodeWarning],
) -> Option<crate::tq::TqAnalysis> {
    if !config.test.enabled {
        return None;
    }
    let declared_fns = crate::dry::collect_declared_functions(parsed);
    let cfg_test_files = crate::dry::dead_code::collect_cfg_test_file_paths(parsed);
    let (prod_calls, test_calls) =
        crate::dry::dead_code::collect_all_calls(parsed, &cfg_test_files);
    let coverage_path = config
        .test
        .coverage_file
        .as_ref()
        .map(std::path::Path::new);
    let ctx = crate::tq::TqContext {
        parsed,
        scope,
        config,
        declared_fns: &declared_fns,
        prod_calls: &prod_calls,
        test_calls: &test_calls,
        all_results,
        dead_code,
        coverage_path,
    };
    Some(crate::tq::analyze_test_quality(&ctx))
}

/// Mark TQ warnings as suppressed based on `// qual:allow(test)` comments.
/// Operation: iteration + suppression check, no own calls.
pub(super) fn mark_tq_suppressions(
    tq: Option<&mut crate::tq::TqAnalysis>,
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
) {
    let Some(tq) = tq else { return };
    let tq_dim = crate::findings::Dimension::Test;
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
pub(super) fn count_tq_warnings(tq: Option<&crate::tq::TqAnalysis>, summary: &mut Summary) {
    let Some(tq) = tq else { return };
    tq.warnings.iter().filter(|w| !w.suppressed).for_each(|w| {
        match &w.kind {
            crate::tq::TqWarningKind::NoAssertion => summary.tq_no_assertion_warnings += 1,
            crate::tq::TqWarningKind::NoSut => summary.tq_no_sut_warnings += 1,
            crate::tq::TqWarningKind::Untested => summary.tq_untested_warnings += 1,
            crate::tq::TqWarningKind::Uncovered => summary.tq_uncovered_warnings += 1,
            crate::tq::TqWarningKind::UntestedLogic { .. } => {
                summary.tq_untested_logic_warnings += 1;
            }
        }
    });
}
