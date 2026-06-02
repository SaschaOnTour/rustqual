//! Tests for Check D — multiplicity mismatch.
//!
//! Check D fires when a target pub-fn IS in every adapter's coverage
//! (so Check B is silent) but the per-adapter handler counts diverge
//! — typical case: cli has two handlers (`cmd_search`, `cmd_grep`)
//! both reaching `session.search` while mcp has only `handle_search`.
//! Split into focused sub-files (each ≤ the SRP file-length cap); shared
//! imports + helpers live here and reach the sub-modules via `use super::*`.

pub(super) use super::support::{
    build_workspace, empty_cfg_test, four_layer, globset, ports_app_cli_mcp, run_check_d, Workspace,
};
pub(super) use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
pub(super) use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
pub(super) use std::collections::HashSet;

/// `(path, source)` file entries for a small workspace fixture.
pub(super) type WsFiles = &'static [(&'static str, &'static str)];
/// Multiplicity case: `(label, files, adapters, target_suffix, expected_counts)`.
/// An empty `expected_counts` means Check D must stay silent.
pub(super) type MultiplicityCase = (
    &'static str,
    WsFiles,
    &'static [&'static str],
    &'static str,
    &'static [(&'static str, usize)],
);
/// Anchor/concrete multiplicity case (ports layout, cli=2 / mcp=1):
/// `(label, files, target)`.
pub(super) type AnchorMultCase = (&'static str, WsFiles, &'static str);

mod anchor_multiplicity;
mod multiplicity;

/// Run Check D with the four-layer layout and no cfg-test files — the
/// shared call across most Check-D tests.
pub(super) fn run_d(ws: &Workspace, cp: &CompiledCallParity) -> Vec<MatchLocation> {
    run_check_d(ws, &four_layer(), cp, &empty_cfg_test())
}

pub(super) fn make_config(adapters: &[&str]) -> CompiledCallParity {
    CompiledCallParity {
        adapters: adapters.iter().map(|s| s.to_string()).collect(),
        target: "application".to_string(),
        call_depth: 3,
        exclude_targets: globset(&[]),
        transparent_wrappers: HashSet::new(),
        transparent_macros: HashSet::new(),
        promoted_attributes: HashSet::new(),
        single_touchpoint: crate::config::architecture::SingleTouchpointMode::default(),
    }
}

pub(super) fn extract_d(findings: &[MatchLocation]) -> Vec<(String, Vec<(String, usize)>)> {
    findings
        .iter()
        .filter_map(|f| match &f.kind {
            ViolationKind::CallParityMultiplicityMismatch {
                target_fn,
                counts_per_adapter,
                ..
            } => Some((target_fn.clone(), counts_per_adapter.clone())),
            _ => None,
        })
        .collect()
}

/// Build a workspace, run Check D under the four-layer layout for the given
/// adapters, and return the multiplicity-mismatch `(target, counts)` pairs.
pub(super) fn multiplicity_4l(
    files: WsFiles,
    adapters: &[&str],
) -> Vec<(String, Vec<(String, usize)>)> {
    extract_d(&run_d(&build_workspace(files), &make_config(adapters)))
}

/// Like [`multiplicity_4l`] but under the hexagonal `ports_app_cli_mcp`
/// layout with the standard `cli + mcp` adapter set.
pub(super) fn multiplicity_ports(files: WsFiles) -> Vec<(String, Vec<(String, usize)>)> {
    extract_d(&run_check_d(
        &build_workspace(files),
        &ports_app_cli_mcp(),
        &make_config(&["cli", "mcp"]),
        &empty_cfg_test(),
    ))
}

/// Look up an adapter's handler count in a multiplicity entry's count list.
pub(super) fn count_for(counts: &[(String, usize)], adapter: &str) -> Option<usize> {
    counts.iter().find(|(a, _)| a == adapter).map(|(_, c)| *c)
}
