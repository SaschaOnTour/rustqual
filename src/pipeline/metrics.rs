use std::collections::HashMap;

use crate::analyzer::FunctionAnalysis;
use crate::config::Config;
use crate::findings::Suppression;
use crate::report::Summary;

/// Compute coupling analysis if enabled.
/// Operation: conditional check + module-qualified call.
pub(super) fn compute_coupling(
    parsed: &[(String, String, syn::File)],
    config: &Config,
) -> Option<crate::coupling::CouplingAnalysis> {
    if !config.coupling.enabled {
        return None;
    }
    Some(crate::coupling::analyze_coupling(parsed))
}

/// Mark coupling metrics as suppressed based on `// qual:allow(coupling)` comments.
/// Operation: iteration + suppression check logic, no own calls.
/// A module is suppressed if ANY of its files contains a coupling suppression.
pub(super) fn mark_coupling_suppressions(
    analysis: Option<&mut crate::coupling::CouplingAnalysis>,
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
) {
    let Some(analysis) = analysis else { return };

    // Collect module names that have coupling suppressions in any of their files
    let suppressed_modules: std::collections::HashSet<String> = suppression_lines
        .iter()
        .filter(|(_, sups)| {
            sups.iter()
                .any(|s| s.covers(crate::findings::Dimension::Coupling))
        })
        .map(|(path, _)| crate::coupling::file_to_module(path))
        .collect();

    for m in &mut analysis.metrics {
        if suppressed_modules.contains(&m.module_name) {
            m.suppressed = true;
        }
    }
}

/// Count coupling warnings and update summary.
/// Operation: iteration + threshold comparison logic, no own calls.
/// Leaf modules (afferent=0) are excluded from instability warnings
/// because I=1.0 is the natural, harmless state for leaf modules.
/// Suppressed modules are excluded from all coupling warnings.
pub(super) fn count_coupling_warnings(
    analysis: Option<&crate::coupling::CouplingAnalysis>,
    config: &crate::config::sections::CouplingConfig,
    summary: &mut Summary,
) {
    let Some(analysis) = analysis else { return };
    for m in &analysis.metrics {
        if m.suppressed {
            continue;
        }
        let instability_exceeded = m.afferent > 0 && m.instability > config.max_instability;
        if instability_exceeded || m.afferent > config.max_fan_in || m.efferent > config.max_fan_out
        {
            summary.coupling_warnings += 1;
        }
    }
    summary.coupling_cycles = analysis.cycles.len();
}

/// Run a detection pass if enabled, returning empty vec when disabled.
/// Operation: conditional guard + closure invocation (lenient).
pub(super) fn run_guarded_detection<T>(
    enabled: bool,
    detect: impl FnOnce(&[(String, String, syn::File)], &Config) -> Vec<T>,
    parsed: &[(String, String, syn::File)],
    config: &Config,
) -> Vec<T> {
    if !enabled {
        return vec![];
    }
    detect(parsed, config)
}

/// Results from all DRY detection passes.
pub(super) struct DryResults {
    pub(super) duplicates: Vec<crate::dry::functions::DuplicateGroup>,
    pub(super) dead_code: Vec<crate::dry::dead_code::DeadCodeWarning>,
    pub(super) fragments: Vec<crate::dry::fragments::FragmentGroup>,
    pub(super) boilerplate: Vec<crate::dry::boilerplate::BoilerplateFind>,
    pub(super) wildcard_warnings: Vec<crate::dry::wildcards::WildcardImportWarning>,
}

/// Run all DRY detection passes (duplicates, dead code, fragments, boilerplate, wildcards).
/// Integration: orchestrates detection sub-functions, no logic.
pub(super) fn run_dry_detection(
    parsed: &[(String, String, syn::File)],
    config: &Config,
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
    api_lines: &std::collections::HashMap<String, std::collections::HashSet<usize>>,
    summary: &mut Summary,
) -> DryResults {
    let duplicates = run_guarded_detection(
        config.duplicates.enabled,
        |p, c| crate::dry::functions::detect_duplicates(p, &c.duplicates),
        parsed,
        config,
    );
    summary.duplicate_groups = duplicates.len();

    let dead_code = run_guarded_detection(
        config.duplicates.enabled,
        |p, c| {
            if !c.duplicates.detect_dead_code {
                return vec![];
            }
            crate::dry::dead_code::detect_dead_code(p, c, api_lines)
        },
        parsed,
        config,
    );
    summary.dead_code_warnings = dead_code.len();

    let fragments = run_guarded_detection(
        config.duplicates.enabled,
        |p, c| crate::dry::fragments::detect_fragments(p, &c.duplicates),
        parsed,
        config,
    );
    summary.fragment_groups = fragments.len();

    let boilerplate = run_guarded_detection(
        config.boilerplate.enabled,
        |p, c| crate::dry::boilerplate::detect_boilerplate(p, &c.boilerplate),
        parsed,
        config,
    );
    summary.boilerplate_warnings = boilerplate.len();

    let mut wildcard_warnings = run_guarded_detection(
        config.duplicates.detect_wildcard_imports,
        |p, c| {
            if !c.duplicates.enabled {
                return vec![];
            }
            crate::dry::wildcards::detect_wildcard_imports(p)
        },
        parsed,
        config,
    );
    mark_wildcard_suppressions(&mut wildcard_warnings, suppression_lines);
    summary.wildcard_import_warnings = wildcard_warnings.iter().filter(|w| !w.suppressed).count();

    DryResults {
        duplicates,
        dead_code,
        fragments,
        boilerplate,
        wildcard_warnings,
    }
}

/// Mark SRP warnings as suppressed based on `// qual:allow(srp)` comments.
/// Operation: iteration + suppression matching via closures (no own calls).
pub(super) fn mark_srp_suppressions(
    srp: Option<&mut crate::srp::SrpAnalysis>,
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
) {
    let Some(srp) = srp else { return };

    // Struct suppressions: comment may be several lines above due to #[derive(...)] attributes
    const SRP_STRUCT_SUPPRESSION_WINDOW: usize = 5;
    let srp_dim = crate::findings::Dimension::Srp;

    srp.struct_warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            w.suppressed = sups.iter().any(|sup| {
                let in_window =
                    sup.line <= w.line && w.line - sup.line <= SRP_STRUCT_SUPPRESSION_WINDOW;
                in_window && sup.covers(srp_dim)
            });
        }
    });

    srp.module_warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            w.suppressed = sups.iter().any(|sup| sup.covers(srp_dim));
        }
    });

    // Param suppressions: proximity-based like struct warnings (attributes above fn)
    srp.param_warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            w.suppressed = sups.iter().any(|sup| {
                let in_window =
                    sup.line <= w.line && w.line - sup.line <= SRP_STRUCT_SUPPRESSION_WINDOW;
                in_window && sup.covers(srp_dim)
            });
        }
    });
}

/// Mark wildcard import warnings as suppressed based on `// qual:allow(dry)` comments.
/// Operation: iteration + suppression check, no own calls.
pub(super) fn mark_wildcard_suppressions(
    warnings: &mut [crate::dry::wildcards::WildcardImportWarning],
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
) {
    let dry_dim = crate::findings::Dimension::Dry;
    warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            w.suppressed = sups.iter().any(|sup| {
                (sup.line == w.line || (w.line > 0 && sup.line == w.line.saturating_sub(1)))
                    && sup.covers(dry_dim)
            });
        }
    });
}

/// Mark SDP violations as suppressed when either involved module has a coupling suppression.
/// Operation: iteration + lookup logic, no own calls.
pub(super) fn mark_sdp_suppressions(
    coupling: Option<&mut crate::coupling::CouplingAnalysis>,
) {
    let Some(coupling) = coupling else { return };
    let suppressed_modules: std::collections::HashSet<&str> = coupling
        .metrics
        .iter()
        .filter(|m| m.suppressed)
        .map(|m| m.module_name.as_str())
        .collect();
    coupling.sdp_violations.iter_mut().for_each(|v| {
        if suppressed_modules.contains(v.from_module.as_str())
            || suppressed_modules.contains(v.to_module.as_str())
        {
            v.suppressed = true;
        }
    });
}

/// Count SDP violations and update summary, excluding suppressed entries.
/// Operation: iteration + conditional counting, no own calls.
pub(super) fn count_sdp_violations(
    coupling: Option<&crate::coupling::CouplingAnalysis>,
    config: &crate::config::sections::CouplingConfig,
    summary: &mut Summary,
) {
    let Some(coupling) = coupling else { return };
    if !config.check_sdp {
        return;
    }
    summary.sdp_violations = coupling
        .sdp_violations
        .iter()
        .filter(|v| !v.suppressed)
        .count();
}

/// Build per-file call graph from IOSP analysis results.
/// Operation: grouping + projection, no own calls.
pub(super) fn build_file_call_graph(
    results: &[FunctionAnalysis],
) -> HashMap<String, Vec<(String, Vec<String>)>> {
    let mut map: HashMap<String, Vec<(String, Vec<String>)>> = HashMap::new();
    for fa in results {
        map.entry(fa.file.clone())
            .or_default()
            .push((fa.name.clone(), fa.own_calls.clone()));
    }
    map
}

/// Compute SRP analysis if enabled.
/// Operation: conditional check + module-qualified call.
pub(super) fn compute_srp(
    parsed: &[(String, String, syn::File)],
    config: &Config,
    file_call_graph: &HashMap<String, Vec<(String, Vec<String>)>>,
) -> Option<crate::srp::SrpAnalysis> {
    if !config.srp.enabled {
        return None;
    }
    Some(crate::srp::analyze_srp(
        parsed,
        &config.srp,
        file_call_graph,
    ))
}

/// Count SRP warnings and update summary, excluding suppressed entries.
/// Operation: iteration + conditional counting, no own calls.
pub(super) fn count_srp_warnings(srp: Option<&crate::srp::SrpAnalysis>, summary: &mut Summary) {
    let Some(srp) = srp else { return };
    summary.srp_struct_warnings = srp.struct_warnings.iter().filter(|w| !w.suppressed).count();
    summary.srp_module_warnings = srp.module_warnings.iter().filter(|w| !w.suppressed).count();
    summary.srp_param_warnings = srp.param_warnings.iter().filter(|w| !w.suppressed).count();
}

/// Populate SRP parameter count warnings from analysis results.
/// Operation: iteration + threshold comparison, no own calls.
pub(super) fn apply_parameter_warnings(
    results: &[crate::analyzer::FunctionAnalysis],
    srp: Option<&mut crate::srp::SrpAnalysis>,
    config: &crate::config::sections::SrpConfig,
) {
    let Some(srp) = srp else { return };
    let max = config.max_parameters;
    srp.param_warnings = results
        .iter()
        .filter(|fa| !fa.suppressed && !fa.is_trait_impl && fa.parameter_count > max)
        .map(|fa| crate::srp::ParamSrpWarning {
            function_name: fa.qualified_name.clone(),
            file: fa.file.clone(),
            line: fa.line,
            parameter_count: fa.parameter_count,
            suppressed: false,
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::{compute_severity, Classification};
    use crate::config::sections::SrpConfig;

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

    fn make_srp() -> crate::srp::SrpAnalysis {
        crate::srp::SrpAnalysis {
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
    fn test_param_warning_suppressed_fn_excluded() {
        let config = SrpConfig::default();
        let mut func = make_func("suppressed_fn", 10, false);
        func.suppressed = true;
        let results = vec![func];
        let mut srp = make_srp();
        apply_parameter_warnings(&results, Some(&mut srp), &config);
        assert!(
            srp.param_warnings.is_empty(),
            "suppressed fn should be excluded"
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

    fn make_sdp_violation(from: &str, to: &str) -> crate::coupling::sdp::SdpViolation {
        crate::coupling::sdp::SdpViolation {
            from_module: from.to_string(),
            to_module: to.to_string(),
            from_instability: 0.2,
            to_instability: 0.8,
            suppressed: false,
        }
    }

    fn make_coupling_metric(name: &str, suppressed: bool) -> crate::coupling::CouplingMetrics {
        crate::coupling::CouplingMetrics {
            module_name: name.to_string(),
            afferent: 1,
            efferent: 1,
            instability: 0.5,
            incoming: vec![],
            outgoing: vec![],
            suppressed,
        }
    }

    #[test]
    fn test_mark_sdp_suppressions_from_module_suppressed() {
        let mut analysis = crate::coupling::CouplingAnalysis {
            metrics: vec![make_coupling_metric("a", true), make_coupling_metric("b", false)],
            cycles: vec![],
            sdp_violations: vec![make_sdp_violation("a", "b")],
        };
        mark_sdp_suppressions(Some(&mut analysis));
        assert!(analysis.sdp_violations[0].suppressed, "from_module suppressed → violation suppressed");
    }

    #[test]
    fn test_mark_sdp_suppressions_to_module_suppressed() {
        let mut analysis = crate::coupling::CouplingAnalysis {
            metrics: vec![make_coupling_metric("a", false), make_coupling_metric("b", true)],
            cycles: vec![],
            sdp_violations: vec![make_sdp_violation("a", "b")],
        };
        mark_sdp_suppressions(Some(&mut analysis));
        assert!(analysis.sdp_violations[0].suppressed, "to_module suppressed → violation suppressed");
    }

    #[test]
    fn test_mark_sdp_suppressions_neither_suppressed() {
        let mut analysis = crate::coupling::CouplingAnalysis {
            metrics: vec![make_coupling_metric("a", false), make_coupling_metric("b", false)],
            cycles: vec![],
            sdp_violations: vec![make_sdp_violation("a", "b")],
        };
        mark_sdp_suppressions(Some(&mut analysis));
        assert!(!analysis.sdp_violations[0].suppressed, "neither suppressed → violation not suppressed");
    }

    #[test]
    fn test_mark_sdp_suppressions_none_coupling() {
        // Should not panic
        mark_sdp_suppressions(None);
    }

    #[test]
    fn test_count_sdp_violations_excludes_suppressed() {
        let analysis = crate::coupling::CouplingAnalysis {
            metrics: vec![],
            cycles: vec![],
            sdp_violations: vec![
                crate::coupling::sdp::SdpViolation {
                    from_module: "a".into(),
                    to_module: "b".into(),
                    from_instability: 0.2,
                    to_instability: 0.8,
                    suppressed: true,
                },
                crate::coupling::sdp::SdpViolation {
                    from_module: "c".into(),
                    to_module: "d".into(),
                    from_instability: 0.3,
                    to_instability: 0.9,
                    suppressed: false,
                },
            ],
        };
        let config = crate::config::sections::CouplingConfig::default();
        let mut summary = Summary::from_results(&[]);
        count_sdp_violations(Some(&analysis), &config, &mut summary);
        assert_eq!(summary.sdp_violations, 1, "Only unsuppressed violations counted");
    }
}
