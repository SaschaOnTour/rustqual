use std::collections::HashMap;

use crate::adapters::analyzers::iosp::FunctionAnalysis;
use crate::config::Config;
use crate::findings::Suppression;
use crate::report::Summary;

/// Compute coupling analysis if enabled.
/// Operation: conditional check + module-qualified call.
pub(super) fn compute_coupling(
    parsed: &[(String, String, syn::File)],
    config: &Config,
) -> Option<crate::adapters::analyzers::coupling::CouplingAnalysis> {
    if !config.coupling.enabled {
        return None;
    }
    Some(crate::adapters::analyzers::coupling::analyze_coupling(
        parsed,
    ))
}

/// Mark coupling metrics as suppressed based on `// qual:allow(coupling)` comments.
/// Operation: iteration + suppression check logic, no own calls.
/// A module is suppressed if ANY of its files contains a coupling suppression.
pub(super) fn mark_coupling_suppressions(
    analysis: Option<&mut crate::adapters::analyzers::coupling::CouplingAnalysis>,
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
        .map(|(path, _)| crate::adapters::analyzers::coupling::file_to_module(path))
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
    analysis: Option<&mut crate::adapters::analyzers::coupling::CouplingAnalysis>,
    config: &crate::config::sections::CouplingConfig,
    summary: &mut Summary,
) {
    let Some(analysis) = analysis else { return };
    for m in &mut analysis.metrics {
        m.warning = false;
        if m.suppressed {
            continue;
        }
        let instability_exceeded = m.afferent > 0 && m.instability > config.max_instability;
        if instability_exceeded || m.afferent > config.max_fan_in || m.efferent > config.max_fan_out
        {
            m.warning = true;
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
    pub(super) duplicates: Vec<crate::adapters::analyzers::dry::functions::DuplicateGroup>,
    pub(super) dead_code: Vec<crate::adapters::analyzers::dry::dead_code::DeadCodeWarning>,
    pub(super) fragments: Vec<crate::adapters::analyzers::dry::fragments::FragmentGroup>,
    pub(super) boilerplate: Vec<crate::adapters::analyzers::dry::boilerplate::BoilerplateFind>,
    pub(super) wildcard_warnings:
        Vec<crate::adapters::analyzers::dry::wildcards::WildcardImportWarning>,
}

/// Run all DRY detection passes (duplicates, dead code, fragments, boilerplate, wildcards).
/// Integration: orchestrates detection sub-functions, no logic.
pub(super) fn run_dry_detection(
    parsed: &[(String, String, syn::File)],
    config: &Config,
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
    api_lines: &std::collections::HashMap<String, std::collections::HashSet<usize>>,
    cfg_test_files: &std::collections::HashSet<String>,
) -> DryResults {
    let duplicates = run_guarded_detection(
        config.duplicates.enabled,
        |p, c| crate::adapters::analyzers::dry::functions::detect_duplicates(p, &c.duplicates),
        parsed,
        config,
    );
    let dead_code = run_guarded_detection(
        config.duplicates.enabled,
        |p, c| {
            if !c.duplicates.detect_dead_code {
                return vec![];
            }
            crate::adapters::analyzers::dry::dead_code::detect_dead_code(
                p,
                c,
                api_lines,
                cfg_test_files,
            )
        },
        parsed,
        config,
    );
    let fragments = run_guarded_detection(
        config.duplicates.enabled,
        |p, c| crate::adapters::analyzers::dry::fragments::detect_fragments(p, &c.duplicates),
        parsed,
        config,
    );
    let boilerplate = run_guarded_detection(
        config.boilerplate.enabled,
        |p, c| crate::adapters::analyzers::dry::boilerplate::detect_boilerplate(p, &c.boilerplate),
        parsed,
        config,
    );
    let mut wildcard_warnings = run_guarded_detection(
        config.duplicates.detect_wildcard_imports,
        |p, c| {
            if !c.duplicates.enabled {
                return vec![];
            }
            crate::adapters::analyzers::dry::wildcards::detect_wildcard_imports(p)
        },
        parsed,
        config,
    );
    mark_wildcard_suppressions(&mut wildcard_warnings, suppression_lines);

    DryResults {
        duplicates,
        dead_code,
        fragments,
        boilerplate,
        wildcard_warnings,
    }
}

/// Count unsuppressed DRY finding entries and update summary.
/// Operation: iteration + filtering + summation on DRY result vectors (no own calls).
pub(super) fn count_dry_findings(
    dry: &DryResults,
    repeated_matches: &[crate::adapters::analyzers::dry::match_patterns::RepeatedMatchGroup],
    summary: &mut Summary,
) {
    summary.dead_code_warnings = dry.dead_code.len();
    summary.wildcard_import_warnings = dry
        .wildcard_warnings
        .iter()
        .filter(|w| !w.suppressed)
        .count();
    summary.duplicate_groups = dry
        .duplicates
        .iter()
        .filter(|g| !g.suppressed)
        .map(|g| g.entries.len())
        .sum();
    summary.fragment_groups = dry
        .fragments
        .iter()
        .filter(|g| !g.suppressed)
        .map(|g| g.entries.len())
        .sum();
    summary.boilerplate_warnings = dry.boilerplate.iter().filter(|b| !b.suppressed).count();
    summary.repeated_match_groups = repeated_matches
        .iter()
        .filter(|g| !g.suppressed)
        .map(|g| g.entries.len())
        .sum();
}

/// Mark SRP warnings as suppressed based on `// qual:allow(srp)` comments.
/// Operation: iteration + suppression matching via closures (no own calls).
pub(super) fn mark_srp_suppressions(
    srp: Option<&mut crate::adapters::analyzers::srp::SrpAnalysis>,
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
    warnings: &mut [crate::adapters::analyzers::dry::wildcards::WildcardImportWarning],
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

/// Count SDP violations and update summary, excluding suppressed entries.
/// Operation: iteration + conditional counting, no own calls.
pub(super) fn count_sdp_violations(
    coupling: Option<&crate::adapters::analyzers::coupling::CouplingAnalysis>,
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
) -> Option<crate::adapters::analyzers::srp::SrpAnalysis> {
    if !config.srp.enabled {
        return None;
    }
    Some(crate::adapters::analyzers::srp::analyze_srp(
        parsed,
        &config.srp,
        file_call_graph,
    ))
}

/// Count SRP warnings and update summary, excluding suppressed entries.
/// Operation: iteration + conditional counting, no own calls.
pub(super) fn count_srp_warnings(
    srp: Option<&crate::adapters::analyzers::srp::SrpAnalysis>,
    summary: &mut Summary,
) {
    let Some(srp) = srp else { return };
    summary.srp_struct_warnings = srp.struct_warnings.iter().filter(|w| !w.suppressed).count();
    summary.srp_module_warnings = srp.module_warnings.iter().filter(|w| !w.suppressed).count();
    summary.srp_param_warnings = srp.param_warnings.iter().filter(|w| !w.suppressed).count();
}

/// Populate SRP parameter count warnings from analysis results.
/// Operation: iteration + threshold comparison, no own calls.
pub(super) fn apply_parameter_warnings(
    results: &[crate::adapters::analyzers::iosp::FunctionAnalysis],
    srp: Option<&mut crate::adapters::analyzers::srp::SrpAnalysis>,
    config: &crate::config::sections::SrpConfig,
) {
    let Some(srp) = srp else { return };
    let max = config.max_parameters;
    srp.param_warnings = results
        .iter()
        .filter(|fa| !fa.suppressed && !fa.is_trait_impl && fa.parameter_count > max)
        .map(|fa| crate::adapters::analyzers::srp::ParamSrpWarning {
            function_name: fa.qualified_name.clone(),
            file: fa.file.clone(),
            line: fa.line,
            parameter_count: fa.parameter_count,
            suppressed: false,
        })
        .collect();
}
