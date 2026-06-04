//! Tests for Check A (adapter-must-delegate).
//!
//! Each test sets up a small multi-file workspace via `build_workspace`,
//! compiles layers + call-parity config, and asserts findings emitted
//! by `check_no_delegation`. Split into focused sub-files (each ≤ the SRP
//! file-length cap); shared imports + helpers live here and reach the
//! sub-modules via `use super::*`.
//!
//! Suppression via `// qual:allow(architecture)` is covered by the
//! golden-example integration test in Task 5 — it piggy-backs on the
//! existing `mark_architecture_suppressions` pipeline and doesn't need
//! a separate unit test here.

pub(super) use super::support::{
    build_workspace, cli_mcp_config, empty_cfg_test, run_check_a, three_layer, Workspace,
};
pub(super) use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};

/// `(path, source)` file entries for a small workspace fixture.
pub(super) type WsFiles = &'static [(&'static str, &'static str)];
/// Delegation case: `(label, files, depth, fn_name, flagged)`.
pub(super) type DelegationCase = (&'static str, WsFiles, usize, &'static str, bool);

mod misc;
mod table;

/// Run Check A with the three-layer layout, a `cli_mcp_config(depth)` config,
/// and no cfg-test files — the shared call across most Check-A tests.
pub(super) fn run_a(ws: &Workspace, depth: usize) -> Vec<MatchLocation> {
    run_check_a(
        ws,
        &three_layer(),
        &cli_mcp_config(depth),
        &empty_cfg_test(),
    )
}

pub(super) fn assert_no_delegation_fn_names(findings: &[MatchLocation]) -> Vec<String> {
    findings
        .iter()
        .filter_map(|f| match &f.kind {
            ViolationKind::CallParityNoDelegation { fn_name, .. } => Some(fn_name.clone()),
            _ => None,
        })
        .collect()
}

/// Build a workspace, run Check A at `depth`, and return the names of
/// adapter fns flagged for no-delegation.
pub(super) fn delegation_names(files: WsFiles, depth: usize) -> Vec<String> {
    assert_no_delegation_fn_names(&run_a(&build_workspace(files), depth))
}
