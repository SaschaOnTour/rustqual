//! Tests for Check B (parity-coverage).
//!
//! Each test sets up a small multi-file workspace and asserts the
//! `missing_adapters` set produced by `check_missing_adapter` for each
//! target-layer pub-fn. Suppression is covered end-to-end in Task 5.

use super::support::{
    build_workspace, cli_mcp_config, empty_cfg_test, four_layer, globset, ports_app_cli_mcp,
    run_check_b, three_layer,
};
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::HashSet;

fn make_config(
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

/// `missing_adapters` list for a specific `target_fn` — `None` when
/// no CallParityMissingAdapter finding exists for that target.
fn missing_adapters_for(findings: &[MatchLocation], target_fn: &str) -> Option<Vec<String>> {
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
fn hint_for(findings: &[MatchLocation], target_fn: &str) -> Option<String> {
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
fn cli_mcp_config_full() -> CompiledCallParity {
    let mut cp = cli_mcp_config(3);
    cp.exclude_targets = globset(&[]);
    cp
}

/// Extract the `(target_fn, missing_adapters)` pair from a
/// CallParityMissingAdapter finding, as `String` for easy assertions.
fn missing_pairs(findings: &[MatchLocation]) -> Vec<(String, Vec<String>)> {
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

// ── Direct / transitive coverage ──────────────────────────────

#[test]
fn test_target_fn_called_from_all_adapters_passes() {
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn cmd_stats() { get_stats(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn handle_stats() { get_stats(); }
            "#,
        ),
        (
            "src/rest/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn post_stats() { get_stats(); }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp", "rest"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    assert!(
        missing_pairs(&findings).is_empty(),
        "covered-by-all should pass, got {findings:?}"
    );
}

#[test]
fn test_target_fn_missing_one_adapter_fails() {
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn cmd_stats() { get_stats(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn handle_stats() { get_stats(); }
            "#,
        ),
        // rest is missing
    ]);
    let cp = make_config(3, &["cli", "mcp", "rest"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    assert_eq!(pairs.len(), 1);
    assert!(pairs[0].0.ends_with("get_stats"));
    assert_eq!(pairs[0].1, vec!["rest".to_string()]);
}

#[test]
fn test_target_fn_missing_two_adapters_fails() {
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn cmd_stats() { get_stats(); }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp", "rest"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    assert_eq!(pairs.len(), 1);
    let mut missing = pairs[0].1.clone();
    missing.sort();
    assert_eq!(missing, vec!["mcp".to_string(), "rest".to_string()]);
}

#[test]
fn test_target_fn_not_called_from_any_adapter_lists_all_missing() {
    let ws = build_workspace(&[("src/application/stats.rs", "pub fn get_stats() {}")]);
    let cp = make_config(3, &["cli", "mcp", "rest"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    assert_eq!(pairs.len(), 1);
    let mut missing = pairs[0].1.clone();
    missing.sort();
    assert_eq!(
        missing,
        vec!["cli".to_string(), "mcp".to_string(), "rest".to_string()]
    );
}

#[test]
fn test_target_fn_reached_transitively_passes() {
    // rest reaches via a service wrapper (depth 2).
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn cmd_stats() { get_stats(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn handle_stats() { get_stats(); }
            "#,
        ),
        (
            "src/rest/service.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn wrap() { get_stats(); }
            "#,
        ),
        (
            "src/rest/handlers.rs",
            r#"
            use crate::rest::service::wrap;
            pub fn post_stats() { wrap(); }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp", "rest"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    assert!(
        missing_pairs(&findings).is_empty(),
        "transitive coverage should pass, got {findings:?}"
    );
}

#[test]
fn test_target_fn_transitive_depth_exceeds_fails() {
    // REST only reaches the target through a layer-less intermediate
    // (`src/shared/` is not a mapped layer), so a shallow call_depth
    // misses it while cli + mcp hit the target directly at depth 1.
    //
    // Backward walk from get_stats:
    //   depth 1: cli::cmd_stats, mcp::handle_stats, shared::deep_call
    //   depth 2: rest::post_stats (through deep_call)
    // With call_depth = 1 the BFS stops at depth 1 → rest missing.
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn cmd_stats() { get_stats(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn handle_stats() { get_stats(); }
            "#,
        ),
        (
            "src/shared/helpers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn deep_call() { get_stats(); }
            "#,
        ),
        (
            "src/rest/handlers.rs",
            r#"
            use crate::shared::helpers::deep_call;
            pub fn post_stats() { deep_call(); }
            "#,
        ),
    ]);
    let cp = make_config(1, &["cli", "mcp", "rest"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    assert_eq!(pairs.len(), 1, "got {findings:?}");
    assert_eq!(pairs[0].1, vec!["rest".to_string()]);
}

#[test]
fn test_target_fn_only_called_from_tests_fails() {
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn cmd_stats() { get_stats(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn handle_stats() { get_stats(); }
            "#,
        ),
        (
            "src/rest/tests.rs",
            r#"
            use crate::application::stats::get_stats;
            #[cfg(test)]
            mod tests {
                use super::*;
                #[test]
                fn test_stats() { get_stats(); }
            }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp", "rest"], &[]);
    let mut cfg_test = HashSet::new();
    cfg_test.insert("src/rest/tests.rs".to_string());
    let findings = run_check_b(&ws, &four_layer(), &cp, &cfg_test);
    let pairs = missing_pairs(&findings);
    assert_eq!(pairs.len(), 1, "got {findings:?}");
    assert_eq!(pairs[0].1, vec!["rest".to_string()]);
}

#[test]
fn test_non_pub_target_fn_ignored() {
    // Private target fn is not part of the parity surface → no finding
    // even when no adapter calls it.
    let ws = build_workspace(&[("src/application/stats.rs", "fn get_stats() {}")]);
    let cp = make_config(3, &["cli", "mcp", "rest"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    assert!(missing_pairs(&findings).is_empty());
}

#[test]
fn test_method_caller_with_receiver_binding_counts() {
    // `let s = Session::open(); s.search()` → receiver tracking resolves
    // the method call to application::session::Session::search, so the
    // mcp adapter counts as reaching `search`.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            impl Session {
                pub fn open() -> Self { Session }
                pub fn search(&self) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn cmd_search() {
                let s = Session::open();
                s.search();
            }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn handle_search() {
                let s = Session::open();
                s.search();
            }
            "#,
        ),
    ]);
    // Only test cli + mcp coverage so rest doesn't appear as missing.
    let cp = make_config(3, &["cli", "mcp"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    // `search` is reached from both adapters; `open` is reached from both.
    assert!(
        missing_pairs(&findings).is_empty(),
        "method-call via binding should count, got {findings:?}"
    );
}

#[test]
fn test_method_caller_without_binding_ignored() {
    // Caller invokes `x.get_stats()` on an unknown-type receiver →
    // `<method>:get_stats`, layer-unknown, so the call doesn't count
    // as reaching `crate::application::stats::get_stats`.
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            pub fn cmd_stats(x: UnknownType) { x.get_stats(); }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].1, vec!["cli".to_string()]);
}

// ── exclude_targets glob ──────────────────────────────────────

#[test]
fn test_target_in_exclude_targets_glob_ignored() {
    let ws = build_workspace(&[
        ("src/application/setup.rs", "pub fn run() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::setup::run;
            pub fn cmd_setup() { run(); }
            "#,
        ),
        // mcp + rest missing, but `setup::run` is excluded.
    ]);
    let cp = make_config(3, &["cli", "mcp", "rest"], &["application::setup::*"]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    assert!(
        missing_pairs(&findings).is_empty(),
        "glob-excluded target should not produce a finding, got {findings:?}"
    );
}

#[test]
fn test_target_not_matching_exclude_glob_still_checked() {
    let ws = build_workspace(&[
        (
            "src/application/setup.rs",
            r#"
            pub fn run() {}
            "#,
        ),
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::setup::run;
            use crate::application::stats::get_stats;
            pub fn cmd_setup() { run(); }
            pub fn cmd_stats() { get_stats(); }
            "#,
        ),
    ]);
    // Exclude only setup::*; stats::get_stats remains in scope.
    let cp = make_config(3, &["cli", "mcp"], &["application::setup::*"]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    assert_eq!(pairs.len(), 1, "got {findings:?}");
    assert!(pairs[0].0.ends_with("get_stats"));
    assert_eq!(pairs[0].1, vec!["mcp".to_string()]);
}

#[test]
fn test_exclude_targets_uses_canonical_without_crate_prefix() {
    // Glob patterns in config don't carry the `crate::` prefix — the
    // matcher needs to strip it before comparing.
    let ws = build_workspace(&[("src/application/setup.rs", "pub fn run() {}")]);
    let cp = make_config(3, &["cli", "mcp"], &["application::setup::run"]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    assert!(missing_pairs(&findings).is_empty());
}

// ── Orphan target-layer islands (v1.2.1) ───────────────────────

#[test]
fn test_orphan_target_with_only_dead_target_caller_fires() {
    // `admin_purge` is a target pub fn that no adapter touches at the
    // boundary. Its only caller in the target layer is another target
    // fn (`_legacy_wrapper`) that is ITSELF unreachable from any
    // adapter — a dead island within the target layer.
    //
    // Before the fix: `has_target_layer_caller` returned true (because
    // `_legacy_wrapper` is a target-layer caller) and the orphan
    // branch was suppressed → no finding. False negative.
    //
    // After: the orphan branch only suppresses when the target is
    // transitively reachable from at least one adapter touchpoint.
    // Since neither admin_purge nor _legacy_wrapper is reachable
    // from any adapter, the orphan finding fires.
    let ws = build_workspace(&[
        (
            "src/application/admin.rs",
            r#"
            pub fn admin_purge() {}
            pub fn _legacy_wrapper() { admin_purge(); }
            "#,
        ),
        // No adapter touches admin_purge or _legacy_wrapper.
        (
            "src/cli/handlers.rs",
            r#"
            pub fn cmd_other() {}
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            pub fn handle_other() {}
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let names: Vec<String> = missing_pairs(&findings)
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    assert!(
        names.iter().any(|n| n.ends_with("admin_purge")),
        "orphan target with only dead target-internal callers must fire, got {names:?}"
    );
}

#[test]
fn test_target_reached_transitively_via_target_chain_no_finding() {
    // Wired chain: cli → session.search (boundary touchpoint) →
    // record_operation (target-internal). record_operation has zero
    // adapter coverage but is reachable through session.search from
    // cli. Must NOT fire (it's not orphan; it's wired).
    let ws = build_workspace(&[
        (
            "src/application/middleware.rs",
            r#"
            pub fn record_operation() {}
            "#,
        ),
        (
            "src/application/session.rs",
            r#"
            use crate::application::middleware::record_operation;
            pub fn search() { record_operation(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn handle_search() { search(); }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let names: Vec<String> = missing_pairs(&findings)
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    assert!(
        !names.iter().any(|n| n.ends_with("record_operation")),
        "transitively-reached target must not fire, got {names:?}"
    );
}

#[test]
fn test_target_self_caller_only_still_fires_orphan() {
    // Self-call: `admin_purge` calls itself recursively. Before the
    // fix, has_target_layer_caller saw `admin_purge` as its own caller
    // (target layer), suppressed the orphan branch. Should fire.
    let ws = build_workspace(&[(
        "src/application/admin.rs",
        r#"
        pub fn admin_purge() { admin_purge(); }
        "#,
    )]);
    let cp = make_config(3, &["cli", "mcp"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let names: Vec<String> = missing_pairs(&findings)
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    assert!(
        names.iter().any(|n| n.ends_with("admin_purge")),
        "self-only-caller orphan must still fire, got {names:?}"
    );
}

// ── Boundary semantic (v1.2.1) ─────────────────────────────────

#[test]
fn test_post_boundary_helper_not_flagged_when_transitive_reach_asymmetric() {
    // The asymmetric setup: cli reaches `record_operation` transitively
    // (via search), mcp doesn't reach it at all (handle_admin → admin,
    // which doesn't touch record_operation). Under the OLD leaf-
    // reachability semantic, this would fire a Check B finding for
    // `record_operation` ("missing from mcp"). Under the new boundary
    // semantic, `record_operation` is application-internal plumbing
    // that no adapter touches at the boundary — not a parity concern.
    let ws = build_workspace(&[
        (
            "src/application/middleware.rs",
            r#"
            pub fn record_operation() {}
            "#,
        ),
        (
            "src/application/session.rs",
            r#"
            use crate::application::middleware::record_operation;
            pub fn search() { record_operation(); }
            pub fn admin() {}
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::admin;
            pub fn handle_admin() { admin(); }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    // `search` is reached only by cli → mismatch finding.
    // `admin` is reached only by mcp → mismatch finding.
    // `record_operation` is post-boundary plumbing → MUST NOT appear.
    let names: Vec<String> = pairs.iter().map(|(t, _)| t.clone()).collect();
    assert!(
        !names.iter().any(|n| n.ends_with("record_operation")),
        "post-boundary helper should not appear in findings, got {pairs:?}"
    );
}

#[test]
fn test_internal_application_chain_no_findings() {
    // session.search → record_operation → impact_count: a chain of
    // internal application fns. Adapters touch only `search`. None of
    // `record_operation` / `impact_count` should produce findings.
    let ws = build_workspace(&[
        (
            "src/application/middleware.rs",
            r#"
            pub fn impact_count() -> u32 { 0 }
            pub fn record_operation() { impact_count(); }
            "#,
        ),
        (
            "src/application/session.rs",
            r#"
            use crate::application::middleware::record_operation;
            pub fn search() { record_operation(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn handle_search() { search(); }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    assert!(
        missing_pairs(&findings).is_empty(),
        "deeper application chain should not fire findings, got {findings:?}"
    );
}

#[test]
fn test_capability_in_one_adapter_only_fires_for_that_target() {
    // cli reaches `admin_purge`; mcp doesn't. mcp reaches `search` too,
    // both adapters cover that one. Only `admin_purge` produces a
    // finding (mismatch case: present in cli, missing from mcp).
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub fn search() {}
            pub fn admin_purge() {}
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::{search, admin_purge};
            pub fn cmd_search() { search(); }
            pub fn cmd_admin() { admin_purge(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn handle_search() { search(); }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    assert_eq!(pairs.len(), 1, "got {findings:?}");
    assert!(
        pairs[0].0.ends_with("admin_purge"),
        "expected admin_purge finding, got {pairs:?}"
    );
    assert_eq!(pairs[0].1, vec!["mcp".to_string()]);
}

#[test]
fn test_impl_in_separate_file_matches_receiver_tracked_calls() {
    // Regression: `use crate::application::session::Session; impl Session
    // { pub fn search() }` in a different file from the type declaration.
    // Without alias-map-based canonicalisation of the impl self-type,
    // Check B's node for `search` would be
    // `crate::application::session_impls::Session::search` while the
    // caller's receiver-tracked canonical is
    // `crate::application::session::Session::search` — the two never
    // match and every adapter looks like it's missing.
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub struct Session;"),
        (
            "src/application/session_impls.rs",
            r#"
            use crate::application::session::Session;
            impl Session {
                pub fn open() -> Self { Session }
                pub fn search(&self) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn cmd_search() {
                let s = Session::open();
                s.search();
            }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn handle_search() {
                let s = Session::open();
                s.search();
            }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp"], &[]);
    let findings = run_check_b(&ws, &four_layer(), &cp, &empty_cfg_test());
    assert!(
        missing_pairs(&findings).is_empty(),
        "cross-file impl via use should match receiver-tracked calls, got {findings:?}"
    );
}

// ── Trait-method anchor coverage ──────────────────────────────

fn ports_cp() -> CompiledCallParity {
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

#[test]
fn check_b_silent_when_anchor_covered_by_all_adapters() {
    // Trait in `ports`, impl in `application` (target). Both CLI and
    // MCP dispatch via `dyn Handler.handle()` — anchor
    // `crate::ports::handler::Handler::handle` is in both adapters'
    // touchpoint sets. The anchor IS a target capability (its impl
    // lives in application), and BOTH adapters reach it → Check B
    // must be silent.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn cmd_dispatch(h: &dyn Handler) { h.handle(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn mcp_dispatch(h: &dyn Handler) { h.handle(); }
            "#,
        ),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let anchor = "crate::ports::handler::Handler::handle";
    assert!(
        !pairs.iter().any(|(target, _)| target == anchor),
        "anchor covered by all adapters must not produce a finding, got {pairs:?}"
    );
}

#[test]
fn check_b_silent_for_concrete_impl_when_only_anchor_reached() {
    // Both adapters dispatch via `dyn Handler.handle()`. The walker
    // registers ONLY the anchor `Handler::handle` as touchpoint —
    // concrete impl methods like `LoggingHandler::handle` never enter
    // the touchpoint set via dispatch. Without F5's anchor-backed-
    // concrete skip, Check B would flag `LoggingHandler::handle` as
    // an orphan even though the trait-method anchor IS covered by
    // both adapters.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn cmd_dispatch(h: &dyn Handler) { h.handle(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn mcp_dispatch(h: &dyn Handler) { h.handle(); }
            "#,
        ),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let concrete = "crate::application::logging::LoggingHandler::handle";
    assert!(
        !pairs.iter().any(|(target, _)| target == concrete),
        "concrete impl-method must NOT be flagged as orphan when its anchor is covered; got {pairs:?}"
    );
}

#[test]
fn check_b_flags_inherent_method_even_when_same_name_inherited_default_exists() {
    // Canonical collision: the inherent method `impl AppHandler { fn
    // handle… }` and the inherited-default `impl Handler for
    // AppHandler {}` share the canonical `AppHandler::handle`. The
    // inherent method has a real body and is a normal target pub-fn
    // — Check B must surface a missing-adapter finding when no
    // adapter reaches it. It must NOT be silently treated as
    // "anchor-backed" via the empty trait impl.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self) {} }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct AppHandler;
            impl Handler for AppHandler {}
            impl AppHandler { pub fn handle(&self) {} }
            "#,
        ),
        ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
        ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let inherent = "crate::application::logging::AppHandler::handle";
    assert!(
        pairs.iter().any(|(target, _)| target == inherent),
        "inherent method `AppHandler::handle` must produce a missing-adapter finding; it must not be silently absorbed into the trait anchor; got {pairs:?}"
    );
}

#[test]
fn check_b_silent_anchor_when_adapters_call_inherited_default_concretely() {
    // Inherited-default impl: every adapter covers the capability via
    // the concrete form (UFCS or struct-method call). The edge-rewrite
    // post-pass folds those phantom edges onto the trait anchor, so
    // coverage hits the anchor for both adapters and the anchor pass
    // stays silent.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self) {} }",
        ),
        (
            "src/application/logging.rs",
            // Empty impl — inherits the trait default body.
            r#"
            use crate::ports::handler::Handler;
            pub struct AppHandler;
            impl Handler for AppHandler {}
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::logging::AppHandler;
            pub fn cmd_log() { AppHandler::handle(&AppHandler); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::logging::AppHandler;
            pub fn mcp_log() { AppHandler::handle(&AppHandler); }
            "#,
        ),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let anchor = "crate::ports::handler::Handler::handle";
    assert!(
        !pairs.iter().any(|(target, _)| target == anchor),
        "anchor must be silent when every adapter covers the capability via the inherited-default concrete form; got {pairs:?}"
    );
}

#[test]
fn check_b_anchor_finding_excluded_via_impl_path_glob() {
    // `exclude_targets = ["application::admin::*"]` matches the impl
    // path; the anchor finding for `ports::Handler::handle` must be
    // silenced via the anchor's backed impl_method_canonicals.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/admin.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct AdminHandler;
            impl Handler for AdminHandler { fn handle(&self) {} }
            "#,
        ),
        ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
        ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
    ]);
    let mut cp = ports_cp();
    cp.exclude_targets = globset(&["application::admin::*"]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &cp, &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let anchor = "crate::ports::handler::Handler::handle";
    assert!(
        !pairs.iter().any(|(target, _)| target == anchor),
        "anchor finding must be silenced when exclude_targets matches an impl_method_canonical (`application::admin::*` matches `application::admin::AdminHandler::handle`); got {pairs:?}"
    );
}

#[test]
fn check_b_silent_anchor_when_all_adapters_cover_via_direct_concrete() {
    // Every adapter calls the concrete impl directly; no dispatch
    // anywhere. The anchor pass must suppress the orphan finding via
    // the impl-method-canonical coverage check.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::logging::LoggingHandler;
            pub fn cmd_log() { LoggingHandler::handle(&LoggingHandler); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::logging::LoggingHandler;
            pub fn mcp_log() { LoggingHandler::handle(&LoggingHandler); }
            "#,
        ),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let anchor = "crate::ports::handler::Handler::handle";
    assert!(
        !pairs.iter().any(|(target, _)| target == anchor),
        "anchor must be silent when the capability is covered via direct concrete by every adapter; got {pairs:?}"
    );
}

#[test]
fn check_b_does_not_skip_concrete_when_an_adapter_calls_it_directly() {
    // Mixed-form drift: cli direct concrete, mcp dispatch. The
    // concrete pass must still run so the form-mismatch surfaces.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::logging::LoggingHandler;
            pub fn cmd_log() {
                LoggingHandler::handle(&LoggingHandler);
            }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn mcp_dispatch(h: &dyn Handler) { h.handle(); }
            "#,
        ),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let concrete = "crate::application::logging::LoggingHandler::handle";
    let concrete_finding = pairs.iter().find(|(t, _)| t == concrete);
    assert!(
        concrete_finding.is_some(),
        "concrete impl-method must NOT be silently skipped when at least one adapter (cli) calls it directly — drift between cli (direct) and mcp (dispatch) must surface; got {pairs:?}"
    );
    if let Some((_, missing)) = concrete_finding {
        assert!(
            missing.iter().any(|a| a == "mcp"),
            "concrete pass must report mcp as missing (it reaches via dispatch, not direct concrete); got {missing:?}"
        );
    }
}

#[test]
fn check_b_flags_anchor_orphan_when_no_adapter_reaches_it() {
    // Trait `Orphan` in ports with overriding impl in application.
    // No adapter dispatches through it. The anchor is a target
    // capability that no adapter wires up → Check B must flag it
    // as orphan/missing-from-all-adapters.
    let ws = build_workspace(&[
        (
            "src/ports/orphan.rs",
            "pub trait Orphan { fn handle(&self); }",
        ),
        (
            "src/application/orphan_impl.rs",
            r#"
            use crate::ports::orphan::Orphan;
            pub struct OrphanImpl;
            impl Orphan for OrphanImpl { fn handle(&self) {} }
            "#,
        ),
        // CLI and MCP exist but never dispatch via `dyn Orphan`.
        ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
        ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let anchor = "crate::ports::orphan::Orphan::handle";
    assert!(
        pairs.iter().any(|(target, _)| target == anchor),
        "anchor reached by NO adapter must be flagged as orphan, got {pairs:?}"
    );
}

#[test]
fn anchor_finding_carries_trait_method_source_line() {
    // Anchor findings must carry the trait method's real file + line
    // (1-based) — line=0 placeholders break suppression-window matching,
    // the orphan detector's window scan, and SARIF startLine validity.
    let ws = build_workspace(&[
        (
            // Lines: 1=blank, 2=trait header, 3=method decl
            "src/ports/orphan.rs",
            "\npub trait Orphan {\n    fn handle(&self);\n}\n",
        ),
        (
            "src/application/orphan_impl.rs",
            r#"
            use crate::ports::orphan::Orphan;
            pub struct OrphanImpl;
            impl Orphan for OrphanImpl { fn handle(&self) {} }
            "#,
        ),
        ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
        ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let anchor = "crate::ports::orphan::Orphan::handle";
    let hit = findings
        .iter()
        .find(|f| match &f.kind {
            ViolationKind::CallParityMissingAdapter { target_fn, .. } => target_fn == anchor,
            _ => false,
        })
        .unwrap_or_else(|| panic!("anchor orphan finding missing, got {findings:?}"));
    assert_eq!(
        hit.file, "src/ports/orphan.rs",
        "anchor finding must carry the trait method's source file, got {hit:?}"
    );
    assert_eq!(
        hit.line, 3,
        "anchor finding must carry the trait method's 1-based line number, got {hit:?}"
    );
    // F6.3 cross-check: anchor finding line is non-zero AND a
    // valid 1-based line number, matching SARIF startLine
    // requirements and what suppression-window matchers expect.
    assert!(
        hit.line >= 1,
        "anchor finding line must be 1-based (>=1), got {}",
        hit.line
    );
}

#[test]
fn check_b_anchor_only_target_surface_still_inspected() {
    // Target layer has NO concrete pub fns — only a default-body trait
    // declared there. The trait method is a target capability via the
    // unified rule. Even though in practice `pub_fns_by_layer["application"]`
    // is created (empty vec) by the collector's `or_default()`, the
    // target-anchor branch must still fire a missing-adapter finding.
    // Sister test below directly exercises the `None` branch via a
    // hand-stripped pub_fns_by_layer.
    let ws = build_workspace(&[
        (
            "src/application/cap.rs",
            "pub trait Cap { fn run(&self) {} }",
        ),
        ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
        ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let anchor = "crate::application::cap::Cap::run";
    assert!(
        pairs.iter().any(|(target, _)| target == anchor),
        "anchor in anchor-only target layer must still fire missing-adapter, got {pairs:?}"
    );
}

#[test]
fn check_b_anchor_reached_transitively_via_target_chain_no_finding() {
    // cli reaches a target fn that dispatches via `dyn Handler`; mcp
    // doesn't reach the anchor at all. The anchor must stay silent
    // (post-boundary plumbing wired via at least one adapter).
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/wires.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            pub fn dispatch(h: &dyn Handler) { h.handle(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::wires::{dispatch, LoggingHandler};
            pub fn cmd_run() { dispatch(&LoggingHandler); }
            "#,
        ),
        ("src/mcp/handlers.rs", "pub fn cmd_other() {}"),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let anchor = "crate::ports::handler::Handler::handle";
    assert!(
        !pairs.iter().any(|(target, _)| target == anchor),
        "anchor reachable transitively via an adapter-touched target fn must be silent (post-boundary plumbing rule), got {pairs:?}"
    );
}

#[test]
fn check_b_anchor_inspected_even_when_target_layer_absent_from_pub_fns_map() {
    // Strips the target entry from pub_fns_by_layer to exercise the
    // `None` branch of `get(target)`. Anchor enumeration must still
    // run.
    use super::support::borrowed_files;
    use crate::adapters::analyzers::architecture::call_parity_rule::build_handler_touchpoints;
    use crate::adapters::analyzers::architecture::call_parity_rule::check_b::check_missing_adapter;
    use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::{
        collect_pub_fns_by_layer, PubFnInputs,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::build_call_graph;

    let ws = build_workspace(&[
        (
            "src/application/cap.rs",
            "pub trait Cap { fn run(&self) {} }",
        ),
        ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
        ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
    ]);
    let layers = ports_app_cli_mcp();
    let cp = ports_cp();
    let cfg_test = empty_cfg_test();
    let borrowed = borrowed_files(&ws);
    let crate_root_modules = HashSet::new();
    let workspace_module_paths = HashSet::new();
    let workspace = crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::WorkspaceLookup {
        cfg_test_files: &cfg_test,
        crate_root_modules: &crate_root_modules,
        workspace_module_paths: &workspace_module_paths,
    };
    let mut pub_fns = collect_pub_fns_by_layer(PubFnInputs {
        files: &borrowed,
        aliases_per_file: &ws.aliases_per_file,
        layers: &layers,
        transparent_wrappers: &cp.transparent_wrappers,
        promoted_attributes: &cp.promoted_attributes,
        workspace: &workspace,
    });
    let graph = build_call_graph(
        &borrowed,
        &ws.aliases_per_file,
        &layers,
        &cp.transparent_wrappers,
        &workspace,
    );
    let touchpoints = build_handler_touchpoints(&pub_fns, &graph, &cp);
    pub_fns.remove("application");
    assert!(
        !pub_fns.contains_key("application"),
        "test precondition: target layer key must be absent before invoking check_missing_adapter"
    );
    let findings = check_missing_adapter(&pub_fns, &graph, &touchpoints, &cp);
    let pairs = missing_pairs(&findings);
    let anchor = "crate::application::cap::Cap::run";
    assert!(
        pairs.iter().any(|(target, _)| target == anchor),
        "with target layer absent from pub_fns_by_layer, anchor enumeration must still run; got {pairs:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// Cascade clearance — when an adapter reaches a generic dispatcher
// that resolves to a trait-anchor edge, downstream callees of the
// impl method must be cleared from missing-adapter findings via the
// forward BFS through target-internal edges.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn check_b_cascade_clears_when_target_reached_via_generic_trait_dispatch() {
    // An adapter reaches a generic runner that dispatches via
    // `Q::execute(...)` to a trait impl. The impl body calls another
    // application helper. With the trait-anchor edge in place, the
    // helper must NOT show up as missing — the BFS from the boundary
    // touchpoints follows the anchor to the impl, then onwards.
    let ws = build_workspace(&[
        (
            "src/application/symbol.rs",
            r#"
            pub trait SymbolQuery {
                fn execute(&self);
            }
            pub struct DepsQuery;
            impl SymbolQuery for DepsQuery {
                fn execute(&self) {
                    collect_callees();
                }
            }
            pub fn collect_callees() {}
            "#,
        ),
        (
            "src/application/runner.rs",
            r#"
            use crate::application::symbol::SymbolQuery;

            pub fn run<Q: SymbolQuery>(q: Q) {
                Q::execute(&q);
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::runner::run;
            use crate::application::symbol::DepsQuery;

            pub fn cmd_deps() {
                run(DepsQuery);
            }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::runner::run;
            use crate::application::symbol::DepsQuery;

            pub fn handle_deps() {
                run(DepsQuery);
            }
            "#,
        ),
    ]);
    let findings = run_check_b(
        &ws,
        &three_layer(),
        &cli_mcp_config_full(),
        &empty_cfg_test(),
    );

    let collect_canonical = "crate::application::symbol::collect_callees";
    let missing = missing_adapters_for(&findings, collect_canonical);

    assert!(
        missing.is_none(),
        "`collect_callees` flagged as not-reached even though it's \
         called by an impl method that an adapter reaches via the \
         generic runner.\n\
         missing adapters for collect_callees: {missing:?}\n\
         all findings: {:?}",
        findings
            .iter()
            .filter_map(|f| match &f.kind {
                ViolationKind::CallParityMissingAdapter { target_fn, .. } =>
                    Some(target_fn.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>(),
    );
}

// ─────────────────────────────────────────────────────────────────────
// Promoted-attribute support — a private attributed fn (e.g.
// `#[tool] async fn ...`) inside an adapter impl block must be
// treated as a handler entry point when configured via
// `promoted_attributes`, so its body's calls count toward coverage.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn check_b_promoted_attribute_lifts_private_fn_onto_handler_surface() {
    // Pattern: `async fn search` without a `pub` modifier inside an
    // inherent impl block — a proc-macro is expected to generate the
    // public dispatch wrapper at expansion time. Without
    // `promoted_attributes` configured, the private fn is filtered
    // out of the adapter's handler set and the call is invisible.
    // With `promoted_attributes = ["tool"]`, the fn is promoted onto
    // the surface and its body's calls satisfy coverage.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            pub struct MyErr;
            impl Session {
                pub fn open(_p: &str) -> Result<Self, MyErr> { todo!() }
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn cmd_search() {
                let _ = Session::open("/p");
            }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            use crate::application::session::{Session, MyErr};

            #[derive(Clone)]
            pub struct Server {
                pub(super) project_root: std::path::PathBuf,
            }

            pub struct Parameters<T>(pub T);
            pub struct SearchParams;
            pub struct CallToolResult;

            #[tool_router(vis = "pub(super)")]
            impl Server {
                #[tool(description = "search")]
                async fn search(
                    &self,
                    _params: Parameters<SearchParams>,
                ) -> Result<CallToolResult, MyErr> {
                    let _session = Session::open("/p");
                    todo!()
                }
            }
            "#,
        ),
    ]);
    let mut cp = cli_mcp_config_full();
    cp.promoted_attributes.insert("tool".to_string());
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());

    let open = "crate::application::session::Session::open";
    let missing = missing_adapters_for(&findings, open);

    assert!(
        missing
            .as_ref()
            .is_none_or(|m| !m.contains(&"mcp".to_string())),
        "with promoted_attributes=[\"tool\"], Session::open is still \
         flagged as not reached from mcp. The promotion in \
         pub_fns.rs::visit_impl_item_fn isn't picking up the #[tool] \
         attribute on the private async fn.\n\
         missing adapters for Session::open: {missing:?}\n\
         all findings: {:?}",
        findings
            .iter()
            .filter_map(|f| match &f.kind {
                ViolationKind::CallParityMissingAdapter { target_fn, .. } =>
                    Some(target_fn.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>(),
    );
}

// ─────────────────────────────────────────────────────────────────────
// Hint surfacing — when a missing adapter has a private fn carrying
// a custom attribute (`#[tool]`, etc.) that would resolve the finding
// after promotion, the finding's `hint` field must name it.
// Negative cases ensure spurious suggestions don't fire.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn check_b_hint_suggests_private_attributed_fn_when_it_reaches_target() {
    // Positive case: missing adapter mcp has a private `#[tool]` fn
    // that transitively reaches the unreached target. Hint must name
    // file + fn + attribute so the author can immediately add
    // `promoted_attributes = ["tool"]`.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            pub struct MyErr;
            impl Session {
                pub fn open(_p: &str) -> Result<Self, MyErr> { todo!() }
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn cmd_search() {
                let _ = Session::open("/p");
            }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            use crate::application::session::{Session, MyErr};
            pub struct Server;
            impl Server {
                #[tool(description = "search")]
                async fn search(&self) -> Result<(), MyErr> {
                    let _ = Session::open("/p");
                    Ok(())
                }
            }
            "#,
        ),
    ]);
    let cp = cli_mcp_config_full();
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());

    let open = "crate::application::session::Session::open";
    let hint = hint_for(&findings, open).expect("expected hint on missing-adapter finding");
    assert!(
        hint.contains("search") && hint.contains("tool") && hint.contains("mcp"),
        "hint must name candidate fn `search`, its attribute `tool`, and \
         adapter `mcp`. Got:\n{hint}"
    );
    assert!(
        hint.contains("promoted_attributes"),
        "hint must point the author at the `promoted_attributes` config knob. Got:\n{hint}"
    );
}

#[test]
fn check_b_no_hint_when_no_private_attributed_fn_in_missing_adapter() {
    // Missing adapter mcp has NO private fn carrying any attribute
    // that reaches the target. Finding still fires but hint is None.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            impl Session {
                pub fn open(_p: &str) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn cmd_search() { Session::open("/p"); }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            pub fn handle_other() {}
            "#,
        ),
    ]);
    let cp = cli_mcp_config_full();
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());

    let open = "crate::application::session::Session::open";
    let missing =
        missing_adapters_for(&findings, open).expect("expected open to be missing from mcp");
    assert!(missing.contains(&"mcp".to_string()));
    assert!(
        hint_for(&findings, open).is_none(),
        "no private attributed candidate in mcp → no hint expected"
    );
}

#[test]
fn check_b_no_hint_when_only_stdlib_attribute_on_candidate() {
    // Private fn carries only stdlib attributes (`#[allow]`,
    // `#[deprecated]`, etc.) — those are noise, not framework-handler
    // markers, so they must not trigger the hint.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            impl Session {
                pub fn open(_p: &str) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn cmd_search() { Session::open("/p"); }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            use crate::application::session::Session;
            pub struct Server;
            impl Server {
                #[allow(dead_code)]
                fn search_internal(&self) {
                    Session::open("/p");
                }
            }
            "#,
        ),
    ]);
    let cp = cli_mcp_config_full();
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());

    let open = "crate::application::session::Session::open";
    assert!(
        hint_for(&findings, open).is_none(),
        "stdlib attribute (#[allow]) must not qualify the fn as a promotion candidate"
    );
}

#[test]
fn check_b_no_hint_when_candidate_body_does_not_reach_target() {
    // Private + attributed fn exists in the missing adapter but its
    // body doesn't reach the unreached target. Promoting it wouldn't
    // resolve the finding, so no hint.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            impl Session {
                pub fn open(_p: &str) {}
                pub fn other(&self) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn cmd_search() { Session::open("/p"); }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            pub struct Server;
            impl Server {
                #[tool(description = "unrelated")]
                async fn unrelated(&self) {
                    // Doesn't call Session::open — calls nothing.
                }
            }
            "#,
        ),
    ]);
    let cp = cli_mcp_config_full();
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());

    let open = "crate::application::session::Session::open";
    assert!(
        hint_for(&findings, open).is_none(),
        "the #[tool] fn `unrelated` doesn't reach Session::open → no hint expected"
    );
}

#[test]
fn check_b_hint_excludes_adapters_already_covering_target() {
    // cli reaches target, mcp doesn't. The hint must mention only
    // mcp's candidate (if any), never cli's — cli already covers the
    // target, so promoting a cli candidate is irrelevant.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            impl Session {
                pub fn stats(&self) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub struct CliCmds;
            impl CliCmds {
                #[tool(description = "diagnostic")]
                fn diagnostic_stats(s: &Session) { s.stats(); }
            }
            pub fn cmd_stats(s: &Session) { s.stats(); }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            pub fn handle_other() {}
            "#,
        ),
    ]);
    let cp = cli_mcp_config_full();
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());

    let stats = "crate::application::session::Session::stats";
    let missing = missing_adapters_for(&findings, stats).expect("stats must be missing from mcp");
    assert_eq!(missing, vec!["mcp".to_string()]);
    // cli has a candidate but cli isn't a missing adapter → no hint.
    assert!(
        hint_for(&findings, stats).is_none(),
        "candidate in cli (already covering) must not surface in the hint when only mcp is missing"
    );
}

#[test]
fn check_b_no_hint_when_candidate_path_exceeds_call_depth() {
    // A private #[tool] fn whose body chain reaches the target only
    // via depth > call_depth must NOT produce a hint — promotion
    // wouldn't help because compute_touchpoints stops at call_depth.
    //
    // call_depth = 1: handler (depth 0) → first callee (depth 1,
    // boundary). With this fixture, `tool_a` reaches `Session::open`
    // only via 2 hops, so even after promotion the walker stops at
    // `helper1` and never sees `open`.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            impl Session { pub fn open() {} }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn cmd_open() { Session::open(); }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            use crate::application::session::Session;
            pub struct Server;
            impl Server {
                #[tool(description = "deep")]
                fn tool_a(&self) { helper1(); }
            }
            fn helper1() { Session::open(); }
            "#,
        ),
    ]);
    let mut cp = cli_mcp_config_full();
    cp.call_depth = 1;
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
    let open = "crate::application::session::Session::open";
    assert!(
        hint_for(&findings, open).is_none(),
        "tool_a → helper1 → open is depth 2, exceeds call_depth=1 — \
         walker would not accept tool_a as touchpoint after promotion, \
         so no hint expected. Got: {:?}",
        hint_for(&findings, open),
    );
}

#[test]
fn check_b_no_hint_when_candidate_reaches_target_only_via_peer_adapter() {
    // A candidate in adapter A whose only path to the target goes
    // through adapter B's code must NOT produce a hint — touchpoint
    // computation from A stops at peer-adapter boundaries, so
    // promotion wouldn't add the target to A's coverage.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            impl Session { pub fn open() {} }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::Session;
            pub fn cli_helper() { Session::open(); }
            pub fn cmd_open() { Session::open(); }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            pub struct Server;
            impl Server {
                #[tool(description = "via peer")]
                fn tool_a(&self) { crate::cli::handlers::cli_helper(); }
            }
            "#,
        ),
    ]);
    let cp = cli_mcp_config_full();
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
    let open = "crate::application::session::Session::open";
    assert!(
        hint_for(&findings, open).is_none(),
        "tool_a reaches open only via cli (peer adapter from mcp) — \
         walker would not traverse past cli boundary after promotion, \
         so no hint expected. Got: {:?}",
        hint_for(&findings, open),
    );
}
