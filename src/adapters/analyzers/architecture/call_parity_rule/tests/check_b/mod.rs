//! Tests for Check B (parity-coverage). Each test sets up a small
//! multi-file workspace and asserts the `missing_adapters` set produced by
//! `check_missing_adapter` for each target-layer pub-fn. Split into focused
//! sub-files (each ≤ the SRP file-length cap); shared imports, case-table
//! types, and run/assert helpers live here and reach the sub-modules via
//! `use super::*`.

pub(super) use super::support::{
    borrowed_files, build_workspace, cli_mcp_config, empty_cfg_test, four_layer, globset,
    ports_app_cli_mcp, run_check_b, three_layer, Workspace,
};
pub(super) use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
pub(super) use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
pub(super) use std::collections::HashSet;

mod anchor_coverage;
mod anchor_edge_cases;
mod coverage;
mod hints;
mod missing_adapter;
mod promotion;
mod visibility;

/// `(path, source)` file entries for a small workspace fixture.
pub(super) type WsFiles = &'static [(&'static str, &'static str)];
/// Trait-anchor coverage case: `(label, files, exclude_glob, target, present)`.
pub(super) type AnchorCase = (
    &'static str,
    WsFiles,
    &'static [&'static str],
    &'static str,
    bool,
);
/// Orphan/boundary visibility case: `(label, files, target_suffix, flagged)`.
pub(super) type VisibilityCase = (&'static str, WsFiles, &'static str, bool);
/// No-finding case: `(label, files, adapters, exclude_glob)`.
pub(super) type SilentCase = (
    &'static str,
    WsFiles,
    &'static [&'static str],
    &'static [&'static str],
);
/// Specific-missing case:
/// `(label, files, depth, adapters, exclude_glob, target_suffix, missing)`.
pub(super) type MissingCase = (
    &'static str,
    WsFiles,
    usize,
    &'static [&'static str],
    &'static [&'static str],
    &'static str,
    &'static [&'static str],
);
/// No-hint case: `(label, files, depth, target)`.
pub(super) type NoHintCase = (&'static str, WsFiles, usize, &'static str);

/// Run Check B with the four-layer layout and no cfg-test files — the
/// shared call across most Check-B tests. Tests needing a different layout,
/// config, or a non-empty cfg-test set call `run_check_b` directly.
pub(super) fn run_b(ws: &Workspace, cp: &CompiledCallParity) -> Vec<MatchLocation> {
    run_check_b(ws, &four_layer(), cp, &empty_cfg_test())
}

pub(super) fn make_config(
    call_depth: usize,
    adapters: &[&str],
    exclude_targets: &[&str],
) -> CompiledCallParity {
    CompiledCallParity {
        adapters: adapters.iter().map(|s| s.to_string()).collect(),
        target: "application".to_string(),
        call_depth,
        exclude_targets: globset(exclude_targets),
        transparent_wrappers: HashSet::new(),
        transparent_macros: HashSet::new(),
        promoted_attributes: HashSet::new(),
        single_touchpoint: crate::config::architecture::SingleTouchpointMode::default(),
    }
}

/// Build the standard stats workspace: `application::get_stats` plus one
/// handler per present adapter (cli→cmd_stats, mcp→handle_stats,
/// rest→post_stats), each delegating to `get_stats`.
pub(super) fn stats_ws(present_adapters: &[&str]) -> Workspace {
    let mut entries = vec![(
        "src/application/stats.rs".to_string(),
        "pub fn get_stats() {}".to_string(),
    )];
    for a in present_adapters {
        let handler = match *a {
            "cli" => "cmd_stats",
            "mcp" => "handle_stats",
            "rest" => "post_stats",
            other => other,
        };
        entries.push((
            format!("src/{a}/handlers.rs"),
            format!(
                "use crate::application::stats::get_stats;\npub fn {handler}() {{ get_stats(); }}"
            ),
        ));
    }
    let refs: Vec<(&str, &str)> = entries
        .iter()
        .map(|(p, s)| (p.as_str(), s.as_str()))
        .collect();
    build_workspace(&refs)
}

/// `missing_adapters` list for a specific `target_fn` — `None` when
/// no CallParityMissingAdapter finding exists for that target.
pub(super) fn missing_adapters_for(
    findings: &[MatchLocation],
    target_fn: &str,
) -> Option<Vec<String>> {
    findings.iter().find_map(|f| match &f.kind {
        ViolationKind::CallParityMissingAdapter {
            target_fn: tf,
            missing_adapters,
            ..
        } if tf == target_fn => Some(missing_adapters.clone()),
        _ => None,
    })
}

/// Hint text for the missing-adapter finding on `target_fn`, if any.
/// Returns `None` if there's no finding or the finding has hint=None.
pub(super) fn hint_for(findings: &[MatchLocation], target_fn: &str) -> Option<String> {
    findings.iter().find_map(|f| match &f.kind {
        ViolationKind::CallParityMissingAdapter {
            target_fn: tf,
            hint,
            ..
        } if tf == target_fn => hint.clone(),
        _ => None,
    })
}

/// Three-layer `cli + mcp + application` config with `application` as
/// target and an empty exclude_targets — the shared shape for the
/// hint/cascade/promoted-attribute tests further down.
pub(super) fn cli_mcp_config_full() -> CompiledCallParity {
    let mut cp = cli_mcp_config(3);
    cp.exclude_targets = globset(&[]);
    cp
}

/// Extract the `(target_fn, missing_adapters)` pair from a
/// CallParityMissingAdapter finding, as `String` for easy assertions.
pub(super) fn missing_pairs(findings: &[MatchLocation]) -> Vec<(String, Vec<String>)> {
    findings
        .iter()
        .filter_map(|f| match &f.kind {
            ViolationKind::CallParityMissingAdapter {
                target_fn,
                missing_adapters,
                ..
            } => Some((target_fn.clone(), missing_adapters.clone())),
            _ => None,
        })
        .collect()
}

// ── Trait-method anchor coverage ──────────────────────────────

pub(super) fn ports_cp() -> CompiledCallParity {
    CompiledCallParity {
        adapters: vec!["cli".to_string(), "mcp".to_string()],
        target: "application".to_string(),
        call_depth: 3,
        exclude_targets: globset(&[]),
        transparent_wrappers: HashSet::new(),
        transparent_macros: HashSet::new(),
        promoted_attributes: HashSet::new(),
        single_touchpoint: crate::config::architecture::SingleTouchpointMode::default(),
    }
}

/// Build a workspace and run Check B with the `ports + application + cli +
/// mcp` layout and the trait-anchor `ports_cp` config — the shared shape
/// for the trait-anchor coverage tests.
pub(super) fn run_b_ports(files: &[(&str, &str)]) -> Vec<MatchLocation> {
    run_check_b(
        &build_workspace(files),
        &ports_app_cli_mcp(),
        &ports_cp(),
        &empty_cfg_test(),
    )
}

/// Build a workspace and run Check B with the three-layer layout and the
/// full `cli + mcp` config (empty exclude) — the shared shape for the
/// cascade / generic-dispatch tests.
pub(super) fn run_b_three(files: &[(&str, &str)]) -> Vec<MatchLocation> {
    run_check_b(
        &build_workspace(files),
        &three_layer(),
        &cli_mcp_config_full(),
        &empty_cfg_test(),
    )
}
