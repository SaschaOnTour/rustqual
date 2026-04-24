//! Tests for Check A (adapter-must-delegate).
//!
//! Each test sets up a small multi-file workspace via `build_workspace`,
//! compiles layers + call-parity config, and asserts findings emitted
//! by `check_no_delegation`.
//!
//! Suppression via `// qual:allow(architecture)` is covered by the
//! golden-example integration test in Task 5 — it piggy-backs on the
//! existing `mark_architecture_suppressions` pipeline and doesn't need
//! a separate unit test here.

use super::support::{borrowed_files, build_workspace, globset, Workspace};
use crate::adapters::analyzers::architecture::call_parity_rule::check_a::check_no_delegation;
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::collect_pub_fns_by_layer;
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::build_call_graph;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use globset::GlobSet;
use std::collections::HashSet;

/// Three-layer test setup: application + cli + mcp.
fn three_layer() -> LayerDefinitions {
    LayerDefinitions::new(
        vec![
            "application".to_string(),
            "cli".to_string(),
            "mcp".to_string(),
        ],
        vec![
            ("application".to_string(), globset(&["src/application/**"])),
            ("cli".to_string(), globset(&["src/cli/**"])),
            ("mcp".to_string(), globset(&["src/mcp/**"])),
        ],
    )
}

fn call_parity_config(call_depth: usize) -> CompiledCallParity {
    CompiledCallParity {
        adapters: vec!["cli".to_string(), "mcp".to_string()],
        target: "application".to_string(),
        call_depth,
        exclude_targets: GlobSet::empty(),
    }
}

fn run_check_a(
    ws: &Workspace,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
) -> Vec<MatchLocation> {
    let borrowed = borrowed_files(ws);
    let cfg_test = HashSet::new();
    let pub_fns = collect_pub_fns_by_layer(&borrowed, &ws.aliases_per_file, layers, &cfg_test);
    let graph = build_call_graph(&borrowed, &ws.aliases_per_file, &cfg_test, layers);
    check_no_delegation(&pub_fns, &graph, layers, cp)
}

fn assert_no_delegation_fn_names(findings: &[MatchLocation]) -> Vec<String> {
    findings
        .iter()
        .filter_map(|f| match &f.kind {
            ViolationKind::CallParityNoDelegation { fn_name, .. } => Some(fn_name.clone()),
            _ => None,
        })
        .collect()
}

// ── Basic direct / inline cases ───────────────────────────────

#[test]
fn test_adapter_fn_direct_delegation_passes() {
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn cmd_stats() {
                get_stats();
            }
            "#,
        ),
    ]);
    let layers = three_layer();
    let cp = call_parity_config(3);
    let findings = run_check_a(&ws, &layers, &cp);
    assert!(
        assert_no_delegation_fn_names(&findings).is_empty(),
        "direct delegation should pass, got {findings:?}"
    );
}

#[test]
fn test_adapter_fn_inline_impl_fails() {
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            pub fn cmd_stats() {
                let _ = 42;
            }
            "#,
        ),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(3));
    let names = assert_no_delegation_fn_names(&findings);
    assert!(
        names.contains(&"cmd_stats".to_string()),
        "inline fn should be flagged, got {names:?}"
    );
}

#[test]
fn test_adapter_fn_transitive_delegation_via_helper_passes() {
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/helpers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn prepare() {
                get_stats();
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::cli::helpers::prepare;
            pub fn cmd_stats() {
                prepare();
            }
            "#,
        ),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(3));
    let names = assert_no_delegation_fn_names(&findings);
    assert!(
        !names.contains(&"cmd_stats".to_string()),
        "transitive delegation at depth 2 should pass, got {names:?}"
    );
}

#[test]
fn test_adapter_fn_transitive_depth_exceeds_limit_fails() {
    // Chain: cmd_stats → h1 → h2 → h3 → h4 → get_stats (5 hops).
    // With call_depth=3 we only explore 3 edges deep → target not reached.
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/helpers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn h4() { get_stats(); }
            pub fn h3() { h4(); }
            pub fn h2() { h3(); }
            pub fn h1() { h2(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::cli::helpers::h1;
            pub fn cmd_stats() { h1(); }
            "#,
        ),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(3));
    let names = assert_no_delegation_fn_names(&findings);
    assert!(
        names.contains(&"cmd_stats".to_string()),
        "depth-exceeding chain should flag cmd_stats, got {names:?}"
    );
}

#[test]
fn test_call_depth_1_only_direct_calls() {
    // cmd_stats calls helper() which calls get_stats.
    // call_depth=1: only direct calls count → helper is not in target → fail.
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/helpers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn helper() { get_stats(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::cli::helpers::helper;
            pub fn cmd_stats() { helper(); }
            "#,
        ),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(1));
    let names = assert_no_delegation_fn_names(&findings);
    assert!(
        names.contains(&"cmd_stats".to_string()),
        "call_depth=1 should flag cmd_stats (helper is not target), got {names:?}"
    );
}

#[test]
fn test_adapter_fn_method_call_does_not_count() {
    // Adapter calls `disp.run(x)` — method call on unknown type, stays
    // `<method>:run` = layer-unknown = no delegation.
    let ws = build_workspace(&[
        ("src/application/dispatch.rs", "pub fn run_it() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            pub fn cmd_stats(disp: UnknownType) {
                disp.run();
            }
            "#,
        ),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(3));
    let names = assert_no_delegation_fn_names(&findings);
    assert!(
        names.contains(&"cmd_stats".to_string()),
        "method call on unknown type must not count as delegation"
    );
}

#[test]
fn test_adapter_fn_cross_adapter_call_does_not_count() {
    // CLI calls an MCP fn (peer, not target) → no delegation credit.
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn handle_stats() { get_stats(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::mcp::handlers::handle_stats;
            pub fn cmd_stats() { handle_stats(); }
            "#,
        ),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(1));
    // At depth 1, cmd_stats only reaches handle_stats (in mcp, not app). → fail.
    let names = assert_no_delegation_fn_names(&findings);
    assert!(
        names.contains(&"cmd_stats".to_string()),
        "cross-adapter call at depth 1 must not count, got {names:?}"
    );
}

#[test]
fn test_adapter_fn_cross_adapter_call_counted_at_deeper_depth() {
    // Sanity check: at depth 2 we reach through mcp into app → passes.
    // This exercises that the graph DOES traverse cross-adapter edges;
    // the prior test ensures one-hop only sees the adapter target.
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn handle_stats() { get_stats(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::mcp::handlers::handle_stats;
            pub fn cmd_stats() { handle_stats(); }
            "#,
        ),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(2));
    let names = assert_no_delegation_fn_names(&findings);
    assert!(
        !names.contains(&"cmd_stats".to_string()),
        "depth=2 reaches app::get_stats transitively, should pass"
    );
}

#[test]
fn test_adapter_fn_cfg_test_file_skipped() {
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            pub fn cmd_stats() {
                let _ = 42;
            }
            "#,
        ),
    ]);
    let borrowed = borrowed_files(&ws);
    let mut cfg_test = HashSet::new();
    cfg_test.insert("src/cli/handlers.rs".to_string());
    let layers = three_layer();
    let pub_fns = collect_pub_fns_by_layer(&borrowed, &ws.aliases_per_file, &layers, &cfg_test);
    let graph = build_call_graph(&borrowed, &ws.aliases_per_file, &cfg_test, &layers);
    let findings = check_no_delegation(&pub_fns, &graph, &layers, &call_parity_config(3));
    assert!(
        findings.is_empty(),
        "cfg-test adapter file must not produce findings, got {findings:?}"
    );
}

#[test]
fn test_adapter_fn_not_in_any_adapter_layer_ignored() {
    // Fn in a layer NOT listed as an adapter → not checked.
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/application/api.rs",
            r#"
            pub fn internal_api() {
                let _ = 42;
            }
            "#,
        ),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(3));
    assert!(
        findings.is_empty(),
        "non-adapter-layer fn must not be checked"
    );
}

#[test]
fn test_finding_line_is_fn_sig_line() {
    let src = "\n\n\npub fn cmd_stats() { let _ = 42; }\n";
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        ("src/cli/handlers.rs", src),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(3));
    let finding = findings
        .iter()
        .find(|f| matches!(f.kind, ViolationKind::CallParityNoDelegation { .. }))
        .expect("expected a CallParityNoDelegation finding");
    // `pub fn cmd_stats` is on line 4 (1-indexed) given 3 leading newlines.
    assert_eq!(
        finding.line, 4,
        "line must anchor on fn sig, got {finding:?}"
    );
    assert_eq!(finding.file, "src/cli/handlers.rs");
}

#[test]
fn test_unparseable_impl_self_type_does_not_collapse_with_free_fns() {
    // Regression: `impl Trait for &dyn Something { fn search() }`'s
    // self-type can't be canonicalised. Previously it was pushed as
    // `Vec::new()`, which made `search` canonicalise to
    // `crate::<file>::search` — colliding with a same-named free fn
    // in the same file and silently polluting the graph. The skip
    // must leave the free fn's node intact.
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn search() { get_stats(); }
            impl dyn std::fmt::Debug {
                // Not a real impl this analyser understands — self-type
                // isn't a plain path, so every method inside must be
                // skipped, not recorded as `crate::cli::handlers::*`.
                pub fn search(&self) {}
            }
            pub fn cmd_x() { search(); }
            "#,
        ),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(3));
    let names = assert_no_delegation_fn_names(&findings);
    assert!(
        !names.contains(&"cmd_x".to_string()),
        "free-fn `search` must remain the node `cmd_x` reaches via delegation, got {names:?}"
    );
}

#[test]
fn test_convergent_graph_does_not_double_enqueue() {
    // Regression guard: if `WalkState::enqueue_unvisited` only checks
    // visited at dequeue (not enqueue), a convergent graph can queue
    // the same node many times. Here 3 helpers all fan out to both
    // `app::a` and `app::b`, and the same callees reach `app::common`.
    // The walk must still terminate (and delegation must resolve)
    // without blowing up the queue.
    let ws = build_workspace(&[
        (
            "src/application/common.rs",
            r#"
            pub fn common() {}
            pub fn a() { common(); }
            pub fn b() { common(); }
            "#,
        ),
        (
            "src/cli/helpers.rs",
            r#"
            use crate::application::common::{a, b};
            pub fn h1() { a(); b(); }
            pub fn h2() { a(); b(); }
            pub fn h3() { a(); b(); }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::cli::helpers::{h1, h2, h3};
            pub fn cmd_x() { h1(); h2(); h3(); }
            "#,
        ),
    ]);
    let findings = run_check_a(&ws, &three_layer(), &call_parity_config(3));
    let names = assert_no_delegation_fn_names(&findings);
    assert!(
        !names.contains(&"cmd_x".to_string()),
        "convergent delegation must still resolve, got {names:?}"
    );
}
