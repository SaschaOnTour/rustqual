//! Tests for Check B (parity-coverage).
//!
//! Each test sets up a small multi-file workspace and asserts the
//! `missing_adapters` set produced by `check_missing_adapter` for each
//! target-layer pub-fn. Suppression is covered end-to-end in Task 5.

use super::support::{
    build_workspace, empty_cfg_test, four_layer, globset, ports_app_cli_mcp, run_check_b,
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
        single_touchpoint: crate::config::architecture::SingleTouchpointMode::default(),
    }
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
fn check_b_anchor_finding_excluded_via_impl_path_glob() {
    // Codex round 3 P2 (2026-05-04): users naturally write
    // `exclude_targets = ["application::admin::*"]` to silence drift
    // on a target-layer feature. For concrete target pub-fns this
    // works because `is_excluded` matches the concrete canonical
    // (`application::admin::AdminHandler::handle`). But the anchor
    // pass tests `is_excluded` only against the ANCHOR canonical
    // (`ports::handler::Handler::handle`) — the impl-path glob
    // never matches → anchor finding fires anyway.
    //
    // Fix: anchor `is_excluded` checks the anchor canonical AND each
    // backed `impl_method_canonical` — if any matches the glob, the
    // anchor finding is silenced. Users now have one consistent
    // exclude form (impl path) that covers both the concrete and the
    // anchor pass, mirroring how `is_anchor_backed_concrete` ties
    // the two together.
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
    // Codex round 3 P1 (2026-05-04): with the conditional concrete
    // skip, all-direct-concrete scenarios already make the concrete
    // pass silent (every adapter covers the concrete target). But
    // the anchor pass in `inspect_anchor` still sees `reached = []`
    // (no adapter has the anchor in coverage) and the reachable BFS
    // doesn't contain the anchor (concrete impl bodies don't call
    // the anchor — they ARE the anchor's implementation). Result:
    // a false-positive anchor orphan finding "missing from all
    // adapters" even though every adapter exercises the capability
    // via direct concrete.
    //
    // Fix: in `inspect_anchor`, when `reached.is_empty()`, also
    // suppress when at least one of `info.impl_method_canonicals`
    // is in some adapter's coverage (or in the reachable set) —
    // the capability IS exercised, just via the concrete form.
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
    // Codex P1 #2 (2026-05-04 review): when one adapter reaches the
    // capability via direct concrete call (`LoggingHandler::handle()`)
    // and another via `dyn Trait` dispatch (anchor), the unconditional
    // `is_anchor_backed_concrete` skip removes the concrete from
    // Check B's iteration entirely. The anchor pass then reports the
    // adapter that called the concrete directly as missing — a false
    // orphan, because that adapter DOES reach the capability, just via
    // the concrete form.
    //
    // Conservative fix: only skip the concrete when NO adapter has it
    // in coverage (all adapters reach via dispatch). When at least one
    // adapter calls the concrete directly, the concrete pass must run
    // — at minimum, the concrete drift becomes visible (the inline
    // documentation acknowledges the resulting double-finding for
    // mixed-form drift; cross-form synonym handling stays out of scope).
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
            // Direct concrete call (UFCS) — emits the concrete canonical
            // unambiguously, regardless of receiver type inference.
            r#"
            use crate::application::logging::LoggingHandler;
            pub fn cmd_log() {
                LoggingHandler::handle(&LoggingHandler);
            }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            // dyn-Trait dispatch — touchpoint is the anchor.
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
    // Codex P2 (2026-05-04 review): the post-boundary reachable BFS
    // only follows callees whose canonical resolves directly to the
    // target layer. A ports-declared trait anchor backed by a target
    // impl has `layer_of(anchor) == "ports"`, so the BFS skips it —
    // even though the anchor IS target capability via the unified rule.
    // Result: an anchor wired up transitively by an adapter (adapter →
    // target fn → `dyn Trait.method()`) gets reported as an orphan
    // because `reachable` doesn't contain it.
    //
    // Setup: cli pub-fn reaches `dispatch` (target fn) which dispatches
    // through `dyn Handler`. mcp doesn't reach the anchor at all.
    // Expected: anchor MUST NOT appear in findings (post-boundary
    // plumbing wired up via at least one adapter is silent per
    // v1.2.1+ semantic).
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
    // Defensive guard for the `None` branch of `pub_fns_by_layer.get(target)`.
    // Codex P1 (2026-05-04 review): even though `or_default()` in the
    // pub-fn collector empirically creates an entry for every layer with
    // ≥1 file, the target-anchor enumeration must NOT depend on that
    // invariant. We strip the target entry from pub_fns_by_layer before
    // calling `check_missing_adapter` to simulate any future refactor
    // (or weird configuration) that could leave the target absent. The
    // anchor capability must still be enumerated and the missing-adapter
    // finding emitted.
    use super::support::borrowed_files;
    use crate::adapters::analyzers::architecture::call_parity_rule::build_handler_touchpoints;
    use crate::adapters::analyzers::architecture::call_parity_rule::check_b::check_missing_adapter;
    use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::collect_pub_fns_by_layer;
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
    let mut pub_fns = collect_pub_fns_by_layer(
        &borrowed,
        &ws.aliases_per_file,
        &layers,
        &cfg_test,
        &cp.transparent_wrappers,
    );
    let graph = build_call_graph(
        &borrowed,
        &ws.aliases_per_file,
        &cfg_test,
        &layers,
        &cp.transparent_wrappers,
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
