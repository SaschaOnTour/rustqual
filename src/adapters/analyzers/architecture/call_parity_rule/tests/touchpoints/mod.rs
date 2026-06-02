//! Tests for `compute_touchpoints` — the boundary-only forward BFS that
//! computes, for one adapter handler, the set of target-layer canonicals it
//! reaches before stopping at the boundary. Split into focused sub-files
//! (each ≤ the SRP file-length cap); shared imports + helpers live here and
//! reach the sub-modules via `use super::*`.

pub(super) use super::support::{
    build_workspace, cli_mcp_config, compute_touchpoints_for, empty_cfg_test, ports_app_cli_mcp,
    three_layer,
};
pub(super) use std::collections::HashSet;

mod anchor_membership;
mod sets;

/// `(path, source)` file entries for a small workspace fixture.
pub(super) type WsFiles = &'static [(&'static str, &'static str)];
/// Touchpoint-set case: `(label, files, depth, handler, expected)`.
pub(super) type TouchpointCase = (
    &'static str,
    WsFiles,
    usize,
    &'static str,
    &'static [&'static str],
);
/// Anchor-membership case: `(label, files, handler, anchor, present)`.
pub(super) type ContainsCase = (&'static str, WsFiles, &'static str, &'static str, bool);

pub(super) fn assert_set(actual: HashSet<String>, expected: &[&str]) {
    let expected_set: HashSet<String> = expected.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        actual, expected_set,
        "touchpoint set mismatch — actual={actual:?} expected={expected_set:?}"
    );
}

/// Compute the touchpoint set for `handler` under the three-layer layout
/// and a `cli_mcp_config(depth)` config (no cfg-test files).
pub(super) fn tp_3l(files: WsFiles, depth: usize, handler: &str) -> HashSet<String> {
    compute_touchpoints_for(
        &build_workspace(files),
        &three_layer(),
        &cli_mcp_config(depth),
        handler,
        &empty_cfg_test(),
    )
}

/// Like [`tp_3l`] but under the hexagonal `ports_app_cli_mcp` layout at
/// call_depth 3 — the shared shape for the trait-anchor membership tests.
pub(super) fn tp_ports(files: WsFiles, handler: &str) -> HashSet<String> {
    compute_touchpoints_for(
        &build_workspace(files),
        &ports_app_cli_mcp(),
        &cli_mcp_config(3),
        handler,
        &empty_cfg_test(),
    )
}
