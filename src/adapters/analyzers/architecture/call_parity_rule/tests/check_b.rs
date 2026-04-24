//! Tests for Check B (parity-coverage).
//!
//! Each test sets up a small multi-file workspace and asserts the
//! `missing_adapters` set produced by `check_missing_adapter` for each
//! target-layer pub-fn. Suppression is covered end-to-end in Task 5.

use super::support::{borrowed_files, build_workspace, globset, Workspace};
use crate::adapters::analyzers::architecture::call_parity_rule::check_b::check_missing_adapter;
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::collect_pub_fns_by_layer;
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::build_call_graph;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::HashSet;

fn three_layer() -> LayerDefinitions {
    LayerDefinitions::new(
        vec![
            "application".to_string(),
            "cli".to_string(),
            "mcp".to_string(),
            "rest".to_string(),
        ],
        vec![
            ("application".to_string(), globset(&["src/application/**"])),
            ("cli".to_string(), globset(&["src/cli/**"])),
            ("mcp".to_string(), globset(&["src/mcp/**"])),
            ("rest".to_string(), globset(&["src/rest/**"])),
        ],
    )
}

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
    }
}

fn run_check_b(
    ws: &Workspace,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
    cfg_test: &HashSet<String>,
) -> Vec<MatchLocation> {
    let borrowed = borrowed_files(ws);
    let pub_fns = collect_pub_fns_by_layer(&borrowed, &ws.aliases_per_file, layers, cfg_test);
    let graph = build_call_graph(&borrowed, &ws.aliases_per_file, cfg_test, layers);
    check_missing_adapter(&pub_fns, &graph, layers, cp)
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

fn empty_cfg_test() -> HashSet<String> {
    HashSet::new()
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &cfg_test);
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
    assert!(missing_pairs(&findings).is_empty());
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
    let findings = run_check_b(&ws, &three_layer(), &cp, &empty_cfg_test());
    assert!(
        missing_pairs(&findings).is_empty(),
        "cross-file impl via use should match receiver-tracked calls, got {findings:?}"
    );
}
