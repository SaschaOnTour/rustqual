//! Tests for `compute_touchpoints` — the boundary-only forward BFS that
//! computes, for one adapter handler, the set of target-layer canonicals
//! it reaches before stopping at the boundary.
//!
//! These tests exercise the algorithm via small multi-file workspaces
//! built through `support::compute_touchpoints_for`, which mirrors the
//! `run_check_a/b` style used elsewhere.

use super::support::{
    build_workspace, cli_mcp_config, compute_touchpoints_for, empty_cfg_test, ports_app_cli_mcp,
    three_layer,
};
use std::collections::HashSet;

fn assert_set(actual: HashSet<String>, expected: &[&str]) {
    let expected_set: HashSet<String> = expected.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        actual, expected_set,
        "touchpoint set mismatch — actual={actual:?} expected={expected_set:?}"
    );
}

// ── Direct call ──────────────────────────────────────────────────

#[test]
fn touchpoints_direct_call() {
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_search",
        &empty_cfg_test(),
    );
    assert_set(touchpoints, &["crate::application::session::search"]);
}

// ── Adapter helper traversed before boundary ─────────────────────

#[test]
fn touchpoints_via_adapter_helper() {
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/helpers.rs",
            r#"
            use crate::application::session::search;
            pub fn format_query() { search(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::cli::helpers::format_query;
            pub fn cmd_search() { format_query(); }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_search",
        &empty_cfg_test(),
    );
    assert_set(touchpoints, &["crate::application::session::search"]);
}

// ── No delegation: handler stays in adapter layer ────────────────

#[test]
fn touchpoints_no_delegation() {
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/helpers.rs",
            r#"
            pub fn adapter_helper2() {}
            pub fn adapter_helper() { adapter_helper2(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::cli::helpers::adapter_helper;
            pub fn cmd_local_only() { adapter_helper(); }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_local_only",
        &empty_cfg_test(),
    );
    assert!(
        touchpoints.is_empty(),
        "no-delegation handler should produce empty touchpoint set, got {touchpoints:?}"
    );
}

// ── Branch with two distinct target functions ────────────────────

#[test]
fn touchpoints_branch_two_targets() {
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub fn foo() {}
            pub fn bar() {}
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::{foo, bar};
            pub fn cmd_branch(cond: bool) {
                if cond { foo(); } else { bar(); }
            }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_branch",
        &empty_cfg_test(),
    );
    assert_set(
        touchpoints,
        &[
            "crate::application::session::foo",
            "crate::application::session::bar",
        ],
    );
}

// ── Loop: same target multiple call sites → single touchpoint ────

#[test]
fn touchpoints_loop_same_target() {
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn process(_x: u32) {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::process;
            pub fn cmd_batch() {
                for x in 0..10 { process(x); }
            }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_batch",
        &empty_cfg_test(),
    );
    assert_set(touchpoints, &["crate::application::session::process"]);
}

// ── Stop at boundary: target callees not in the set ──────────────

#[test]
fn touchpoints_stop_at_target_boundary() {
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            use crate::application::middleware::record_operation;
            pub fn search() { record_operation(); }
            "#,
        ),
        (
            "src/application/middleware.rs",
            r#"
            pub fn record_operation() {}
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_search",
        &empty_cfg_test(),
    );
    // Only session::search — the boundary semantic stops here. The
    // application-internal `record_operation` MUST NOT appear in the set.
    assert_set(touchpoints, &["crate::application::session::search"]);
}

// ── Depth limit: chain too deep returns empty ────────────────────

#[test]
fn touchpoints_call_depth_exceeded() {
    // cmd → h1 → h2 → h3 → h4 → search (5 hops). depth=3 stops short.
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/helpers.rs",
            r#"
            use crate::application::session::search;
            pub fn h4() { search(); }
            pub fn h3() { h4(); }
            pub fn h2() { h3(); }
            pub fn h1() { h2(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::cli::helpers::h1;
            pub fn cmd_deep() { h1(); }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_deep",
        &empty_cfg_test(),
    );
    assert!(
        touchpoints.is_empty(),
        "depth=3 should not reach a 5-hop target, got {touchpoints:?}"
    );
}

// ── Depth limit: chain just inside limit returns target ──────────

#[test]
fn touchpoints_call_depth_just_inside() {
    // cmd → h1 → h2 → search (3 hops). depth=3 reaches target.
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/helpers.rs",
            r#"
            use crate::application::session::search;
            pub fn h2() { search(); }
            pub fn h1() { h2(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::cli::helpers::h1;
            pub fn cmd_depth3() { h1(); }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_depth3",
        &empty_cfg_test(),
    );
    assert_set(touchpoints, &["crate::application::session::search"]);
}

// ── Trait-dispatch anchor ────────────────────────────────────────

#[test]
fn touchpoints_trait_dispatch_collapses_to_anchor() {
    // CLI handler calls `dyn Handler.handle()` where Handler has THREE
    // overriding impls in the application layer. Touchpoint set must be
    // ONE anchor `<Trait>::<method>`, not three impl edges. Otherwise
    // Check C fires multi-touchpoint for what is semantically a single
    // boundary call.
    let ws = build_workspace(&[
        (
            "src/application/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            pub struct MetricsHandler;
            impl Handler for MetricsHandler { fn handle(&self) {} }
            pub struct AuditHandler;
            impl Handler for AuditHandler { fn handle(&self) {} }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::handler::Handler;
            pub fn cmd_dispatch(h: &dyn Handler) { h.handle(); }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_dispatch",
        &empty_cfg_test(),
    );
    assert_set(
        touchpoints,
        &["crate::application::handler::Handler::handle"],
    );
}

#[test]
fn touchpoints_trait_anchor_recognized_when_trait_lives_in_ports_layer() {
    // Hexagonal/Ports&Adapters case: trait declared in `ports` layer,
    // impls in `application` (target) layer. Dispatch emits anchor
    // `crate::ports::Handler::handle` (anchor lives in ports). Walker
    // must still register the anchor as a target boundary because
    // its impls reach the target layer — otherwise Check A would
    // falsely fire "no delegation" for a CLI command that legitimately
    // crosses into the target via trait dispatch.
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
    ]);
    let mut config = cli_mcp_config(3);
    config.target = "application".to_string();
    let touchpoints = compute_touchpoints_for(
        &ws,
        &ports_app_cli_mcp(),
        &config,
        "cmd_dispatch",
        &empty_cfg_test(),
    );
    assert_set(touchpoints, &["crate::ports::handler::Handler::handle"]);
}

#[test]
fn touchpoints_skip_anchor_declared_in_peer_adapter_layer() {
    // Trait declared in peer-adapter layer (`mcp`), with an overriding
    // impl in the target layer (`application`). CLI handler calls
    // `dyn mcp::Handler.handle()`. The anchor lives in `mcp` (peer
    // adapter), so the walker MUST refuse to register it as a target
    // boundary — otherwise CLI inherits MCP's reachability into
    // application via the anchor and fakes adapter delegation.
    // Regression for the v1.2.2 regression that anchor promotion
    // bypassed the existing peer-adapter filter (review pass
    // 2026-05-04 P2 #2).
    let ws = build_workspace(&[
        (
            "src/mcp/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::mcp::handler::Handler;
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::mcp::handler::Handler;
            pub fn cmd_via_dyn_peer(h: &dyn Handler) { h.handle(); }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_via_dyn_peer",
        &empty_cfg_test(),
    );
    assert_set(touchpoints, &[]);
}

// ── Peer-adapter blocking ────────────────────────────────────────

#[test]
fn touchpoints_do_not_traverse_peer_adapter() {
    // CLI handler `cmd_via_mcp` calls into an MCP handler `mcp_search`,
    // which itself crosses into the application layer. The CLI walk
    // must NOT inherit MCP's touchpoint — otherwise Check A/B/D would
    // be masked even though the CLI adapter never crossed into the
    // target itself.
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn mcp_search() { search(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::mcp::handlers::mcp_search;
            pub fn cmd_via_mcp() { mcp_search(); }
            "#,
        ),
    ]);
    let touchpoints = compute_touchpoints_for(
        &ws,
        &three_layer(),
        &cli_mcp_config(3),
        "cmd_via_mcp",
        &empty_cfg_test(),
    );
    assert_set(touchpoints, &[]);
}
