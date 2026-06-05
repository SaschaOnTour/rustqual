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

/// Count coupling warnings and update summary. Each module-level metric
/// (instability / fan-in / fan-out) is gated by a blanket or targeted
/// `allow(coupling, …)`; leaf modules (afferent=0) are instability-exempt.
/// Operation: iterates metrics, delegating the per-module decision.
pub(super) fn count_coupling_warnings(
    analysis: Option<&mut crate::adapters::analyzers::coupling::CouplingAnalysis>,
    config: &crate::config::sections::CouplingConfig,
    sups: &crate::app::coupling_suppressions::ModuleCouplingSuppressions,
    summary: &mut Summary,
) {
    let Some(analysis) = analysis else { return };
    for m in &mut analysis.metrics {
        m.warning = module_coupling_warns(m, config, sups);
        if m.warning {
            summary.coupling_warnings += 1;
        }
    }
    summary.coupling_cycles = analysis.cycles.len();
}

/// Whether a module has any unsuppressed coupling metric breach.
/// Operation: per-metric checks reaching the suppression helper.
fn module_coupling_warns(
    m: &crate::adapters::analyzers::coupling::CouplingMetrics,
    config: &crate::config::sections::CouplingConfig,
    sups: &crate::app::coupling_suppressions::ModuleCouplingSuppressions,
) -> bool {
    let inst = m.afferent > 0
        && m.instability > config.max_instability
        && !sups.suppresses(&m.module_name, "max_instability", Some(m.instability));
    let fan_in = m.afferent > config.max_fan_in
        && !sups.suppresses(&m.module_name, "max_fan_in", Some(m.afferent as f64));
    let fan_out = m.efferent > config.max_fan_out
        && !sups.suppresses(&m.module_name, "max_fan_out", Some(m.efferent as f64));
    inst || fan_in || fan_out
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

/// Per-file line numbers of the two annotation markers that DRY/TQ care
/// about: `// qual:api` (excludes from dead-code + untested) and
/// `// qual:test_helper` (same exclusions, different rationale).
pub(super) struct AnnotationLines<'a> {
    pub(super) api: &'a std::collections::HashMap<String, std::collections::HashSet<usize>>,
    pub(super) test_helper: &'a std::collections::HashMap<String, std::collections::HashSet<usize>>,
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
    annotation_lines: &AnnotationLines<'_>,
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
                annotation_lines.api,
                annotation_lines.test_helper,
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

/// Mark wildcard import warnings as suppressed based on `// qual:allow(dry)` comments.
/// Operation: iteration + suppression check, no own calls.
pub(super) fn mark_wildcard_suppressions(
    warnings: &mut [crate::adapters::analyzers::dry::wildcards::WildcardImportWarning],
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
) {
    let dry_dim = crate::findings::Dimension::Dry;
    // Window width shared with the orphan detector, see
    // `app::suppression_windows::WILDCARD`.
    let window = super::suppression_windows::WILDCARD;
    warnings.iter_mut().for_each(|w| {
        if let Some(sups) = suppression_lines.get(&w.file) {
            w.suppressed = sups.iter().any(|sup| {
                sup.line <= w.line
                    && w.line - sup.line <= window
                    && sup.suppresses(dry_dim, "wildcard_imports", None)
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
    let test_file_length = config.tests.file_length.unwrap_or(config.srp.file_length);
    Some(crate::adapters::analyzers::srp::analyze_srp(
        parsed,
        &config.srp,
        file_call_graph,
        test_file_length,
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
    // Emit warnings for every over-threshold function and mark
    // suppressed ones explicitly; don't filter suppressed out of the
    // collection. This way the orphan-suppression checker can see that
    // a `// qual:allow(srp)` marker did have a target (it just
    // preemptively silenced the finding).
    srp.param_warnings = results
        .iter()
        .filter(|fa| !fa.is_trait_impl && fa.parameter_count > max)
        .map(|fa| crate::adapters::analyzers::srp::ParamSrpWarning {
            function_name: fa.qualified_name.clone(),
            file: fa.file.clone(),
            line: fa.line,
            parameter_count: fa.parameter_count,
            suppressed: fa.suppressed,
        })
        .collect();
}
