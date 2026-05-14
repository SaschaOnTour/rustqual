//! Evaluation tests for the five call_parity / suppression gaps in
//! rustqual 1.2.2 surfaced by the rlm post-Slice-0.8 audit.
//!
//! These tests are failing-first regression probes. Their job is to
//! prove or disprove each of the user's hypothesised mechanisms
//! inside rustqual's own test harness so a subsequent fix plan can
//! target the real root cause.
//!
//! Bug groups (verified causes from the user's report 2026-05-13):
//!
//! - **bug1a / bug1b / bug1c / bug1a-async / bug2 / bug2-match** —
//!   the originally-suspected mechanisms (self-helper chain,
//!   receiver-source asymmetry, generic-to-generic propagation).
//!   ALL PASS — kept as documentation that those hypotheses were
//!   wrong.
//! - **bug1_tool_router** — `#[tool_router]` / `#[tool]` attribute on
//!   impl block + `#[tool]` on method + `Parameters<T>` argument
//!   shape (the actual rmcp pattern). Verified cause from rlm-side
//!   investigation: calls inside this impl block aren't traced.
//! - **bug2_pub_use** — caller imports callee via a parent-module
//!   `pub use` re-export. Verified cause: rustqual canonicalises the
//!   call site to the re-export path which has no fn definition →
//!   edge missing.
//! - **bug3** — cascade clearance gated on bug4 fix.
//! - **bug4** — `Q::method()` where `Q: Trait` emits a trait-anchor
//!   edge `<Trait>::<method>`.
//! - **bug5_unknown_dim** — `// qual:allow(srp_params)` (unknown
//!   dimension argument) silently falls back to the global-suppress
//!   form. Direct parser test, no call_parity involvement.

use super::support::{
    build_graph_only, build_workspace, cli_mcp_config, empty_cfg_test, globset, run_check_b,
    three_layer,
};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::CallGraph;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::HashSet;

// ─────────────────────────────────────────────────────────────────────
// Helpers shared across the bug evaluations
// ─────────────────────────────────────────────────────────────────────

fn graph_contains_edge(graph: &CallGraph, from: &str, to: &str) -> bool {
    graph
        .forward
        .get(from)
        .is_some_and(|callees| callees.contains(to))
}

fn callees_of<'g>(graph: &'g CallGraph, from: &str) -> Vec<&'g str> {
    graph
        .forward
        .get(from)
        .map(|set| {
            let mut v: Vec<&str> = set.iter().map(String::as_str).collect();
            v.sort();
            v
        })
        .unwrap_or_default()
}

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

fn rlm_layers_for_eval() -> LayerDefinitions {
    three_layer()
}

fn rlm_cfg_for_eval() -> CompiledCallParity {
    let mut cp = cli_mcp_config(3);
    cp.exclude_targets = globset(&[]);
    cp
}

// ═══════════════════════════════════════════════════════════════════
// Bug 1a — `self.helper()` chain to associated-fn call on target type
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug1a_self_helper_chain_to_associated_fn() {
    // Mirrors rlm/mcp:
    //   impl RlmServer {
    //       pub(crate) fn ensure_session(&self) -> Result<RlmSession, _> {
    //           RlmSession::open("/p")
    //       }
    //   }
    //   impl RlmServer {
    //       pub fn search(&self) -> Result<(), _> { self.ensure_session()?; Ok(()) }
    //   }
    //
    // Asserts both edges exist in the workspace graph:
    //   RlmServer::search → RlmServer::ensure_session
    //   RlmServer::ensure_session → RlmSession::open
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            pub struct MyErr;
            impl RlmSession {
                pub fn open(_p: &str) -> Result<Self, MyErr> { todo!() }
            }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            use crate::application::session::{RlmSession, MyErr};

            pub struct RlmServer;

            impl RlmServer {
                pub(crate) fn ensure_session(&self) -> Result<RlmSession, MyErr> {
                    RlmSession::open("/p")
                }
            }

            impl RlmServer {
                pub fn search(&self) -> Result<(), MyErr> {
                    self.ensure_session()?;
                    Ok(())
                }
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );

    let search = "crate::mcp::server::RlmServer::search";
    let ensure = "crate::mcp::server::RlmServer::ensure_session";
    let open = "crate::application::session::RlmSession::open";

    let edge1 = graph_contains_edge(&graph, search, ensure);
    let edge2 = graph_contains_edge(&graph, ensure, open);

    assert!(
        edge1 && edge2,
        "bug1a — chain `search → ensure_session → RlmSession::open` broken.\n\
         search → ensure_session: {edge1}\n\
         ensure_session → RlmSession::open: {edge2}\n\
         search callees: {:?}\n\
         ensure_session callees: {:?}",
        callees_of(&graph, search),
        callees_of(&graph, ensure),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 1b — locally-bound `let session = free_fn()?` traces N method calls
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug1b_locally_bound_receiver_traces_multi_method() {
    // Mirrors rlm/cli:
    //   pub fn cmd_replace(preview: bool) -> Result<(), _> {
    //       let session = open_session_in_cwd()?;
    //       if preview { session.replace_preview(); }
    //       else { session.replace_apply(); }
    //       Ok(())
    //   }
    //
    // Asserts BOTH method calls produce edges from cmd_replace.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            pub struct MyErr;
            impl RlmSession {
                pub fn replace_preview(&self) {}
                pub fn replace_apply(&self) {}
            }
            "#,
        ),
        (
            "src/cli/helpers.rs",
            r#"
            use crate::application::session::{RlmSession, MyErr};
            pub fn open_session_in_cwd() -> Result<RlmSession, MyErr> { todo!() }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::cli::helpers::open_session_in_cwd;
            use crate::application::session::MyErr;

            pub fn cmd_replace(preview: bool) -> Result<(), MyErr> {
                let session = open_session_in_cwd()?;
                if preview {
                    session.replace_preview();
                } else {
                    session.replace_apply();
                }
                Ok(())
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );

    let cmd = "crate::cli::handlers::cmd_replace";
    let preview = "crate::application::session::RlmSession::replace_preview";
    let apply = "crate::application::session::RlmSession::replace_apply";

    let has_preview = graph_contains_edge(&graph, cmd, preview);
    let has_apply = graph_contains_edge(&graph, cmd, apply);

    assert!(
        has_preview && has_apply,
        "bug1b — locally-bound `session.X()` calls not traced.\n\
         cmd_replace → replace_preview: {has_preview}\n\
         cmd_replace → replace_apply:   {has_apply}\n\
         cmd_replace callees: {:?}",
        callees_of(&graph, cmd),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 1c — parameter-bound receiver (control: should PASS)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug1c_param_bound_receiver_traces_multi_method_control() {
    // Mirrors rlm/mcp:
    //   pub fn handle_replace(session: &RlmSession, preview: bool) {
    //       if preview { session.replace_preview(); }
    //       else { session.replace_apply(); }
    //   }
    //
    // Control test — expected to PASS under HEAD. If it fails, the
    // bug is broader than the receiver-source asymmetry.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            impl RlmSession {
                pub fn replace_preview(&self) {}
                pub fn replace_apply(&self) {}
            }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::RlmSession;

            pub fn handle_replace(session: &RlmSession, preview: bool) {
                if preview {
                    session.replace_preview();
                } else {
                    session.replace_apply();
                }
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );

    let h = "crate::mcp::handlers::handle_replace";
    let preview = "crate::application::session::RlmSession::replace_preview";
    let apply = "crate::application::session::RlmSession::replace_apply";

    let has_preview = graph_contains_edge(&graph, h, preview);
    let has_apply = graph_contains_edge(&graph, h, apply);

    assert!(
        has_preview && has_apply,
        "bug1c (control) — parameter-bound `session.X()` calls not traced.\n\
         handle_replace → replace_preview: {has_preview}\n\
         handle_replace → replace_apply:   {has_apply}\n\
         handle_replace callees: {:?}",
        callees_of(&graph, h),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 2 — generic-fn body emits direct path-call edge for inner call
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug2_generic_to_generic_path_call_edge() {
    // Mirrors rlm/middleware → savings:
    //   pub fn record_operation<T: Serialize>(meta: &str, result: &T) {
    //       savings::record_file_op(result, meta);
    //   }
    //   pub fn record_file_op<T: Serialize>(_: &T, _: &str) {}
    //
    // The user's hypothesis: "generic-to-generic chain breaks".
    // Counter-hypothesis (from static read): the call is a plain
    // `Path` call canonicalised through the `savings` alias, so the
    // edge SHOULD be created regardless of `T`. This test settles it.
    let ws = build_workspace(&[
        (
            "src/application/savings.rs",
            r#"
            pub fn record_file_op<T>(_r: &T, _meta: &str) {}
            "#,
        ),
        (
            "src/application/middleware.rs",
            r#"
            use crate::application::savings;

            pub fn record_operation<T>(meta: &str, result: &T) {
                savings::record_file_op(result, meta);
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );

    let outer = "crate::application::middleware::record_operation";
    let inner = "crate::application::savings::record_file_op";

    assert!(
        graph_contains_edge(&graph, outer, inner),
        "bug2 — direct path-call edge from generic `record_operation` \
         to generic `record_file_op` missing.\n\
         record_operation callees: {:?}",
        callees_of(&graph, outer),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 4 — `Q::method()` where `Q: Trait` emits trait-anchor edge
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug4_generic_param_method_call_emits_trait_anchor_edge() {
    // Mirrors rlm/symbol:
    //   pub trait SymbolQuery { fn execute(&self); }
    //   impl SymbolQuery for DepsQuery { fn execute(&self) {} }
    //   pub fn run<Q: SymbolQuery>(q: Q) { Q::execute(&q); }
    //
    // Expects `run`'s edges to contain the trait anchor
    //   `crate::application::symbol::SymbolQuery::execute`
    // (the form `populate_anchor_index` registers).
    //
    // High-confidence FAIL: `canonicalise_path` has no branch for
    // generic type parameters with trait bounds; segments[0]="Q"
    // falls through to `bare("Q::execute")`.
    let ws = build_workspace(&[
        (
            "src/application/symbol.rs",
            r#"
            pub trait SymbolQuery {
                fn execute(&self);
            }
            pub struct DepsQuery;
            impl SymbolQuery for DepsQuery {
                fn execute(&self) {}
            }
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
    ]);
    let graph = build_graph_only(
        &ws,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );

    let runner = "crate::application::runner::run";
    let anchor = "crate::application::symbol::SymbolQuery::execute";
    let impl_method = "crate::application::symbol::DepsQuery::execute";

    let has_anchor = graph_contains_edge(&graph, runner, anchor);
    let has_impl = graph_contains_edge(&graph, runner, impl_method);

    // Sanity check: same shape with `where`-clause spelling must
    // produce the same edge — both forms are semantically identical
    // and Q::method() dispatch shouldn't depend on bound placement.
    let ws_where = build_workspace(&[
        (
            "src/application/symbol.rs",
            r#"
            pub trait SymbolQuery { fn execute(&self); }
            pub struct DepsQuery;
            impl SymbolQuery for DepsQuery { fn execute(&self) {} }
            "#,
        ),
        (
            "src/application/runner.rs",
            r#"
            use crate::application::symbol::SymbolQuery;
            pub fn run<Q>(q: Q) where Q: SymbolQuery { Q::execute(&q); }
            "#,
        ),
    ]);
    let graph_where = build_graph_only(
        &ws_where,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );
    assert!(
        graph_contains_edge(&graph_where, runner, anchor),
        "where-clause variant must emit the same trait-anchor edge as inline-bound. \
         run callees: {:?}",
        callees_of(&graph_where, runner),
    );

    assert!(
        has_anchor || has_impl,
        "bug4 — `Q::execute(&q)` emits no edge to either the trait \
         anchor `{anchor}` or the impl method `{impl_method}`.\n\
         run callees: {:?}",
        callees_of(&graph, runner),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 1a-async — same as bug1a but with async fn signatures
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug1a_async_self_helper_chain() {
    // Mirrors rmcp's `#[tool] async fn` shape more closely.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            pub struct MyErr;
            impl RlmSession {
                pub fn open(_p: &str) -> Result<Self, MyErr> { todo!() }
            }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            use crate::application::session::{RlmSession, MyErr};

            pub struct RlmServer;

            impl RlmServer {
                pub(crate) fn ensure_session(&self) -> Result<RlmSession, MyErr> {
                    RlmSession::open("/p")
                }
            }

            impl RlmServer {
                pub async fn search(&self) -> Result<(), MyErr> {
                    self.ensure_session()?;
                    Ok(())
                }
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );

    let search = "crate::mcp::server::RlmServer::search";
    let ensure = "crate::mcp::server::RlmServer::ensure_session";
    let open = "crate::application::session::RlmSession::open";

    let edge1 = graph_contains_edge(&graph, search, ensure);
    let edge2 = graph_contains_edge(&graph, ensure, open);

    assert!(
        edge1 && edge2,
        "bug1a-async — chain broken with async fn.\n\
         search → ensure_session: {edge1}\n\
         ensure_session → RlmSession::open: {edge2}\n\
         search callees: {:?}\n\
         ensure_session callees: {:?}",
        callees_of(&graph, search),
        callees_of(&graph, ensure),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 2-match — same as bug2 but with the `match` arm shape
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug2_generic_match_arm_emits_edge() {
    // The exact rlm shape: outer call inside a match-arm body.
    let ws = build_workspace(&[
        (
            "src/application/savings.rs",
            r#"
            pub fn record_file_op<T>(_r: &T, _meta: &str) {}
            pub fn record_symbol_op<T>(_r: &T, _meta: &str) {}
            "#,
        ),
        (
            "src/application/middleware.rs",
            r#"
            use crate::application::savings;

            pub enum AlternativeCost { SingleFile(String), SymbolFiles(String) }

            pub fn record_operation<T>(meta: &str, result: &T, alt: &AlternativeCost) {
                let _ = match alt {
                    AlternativeCost::SingleFile(p) => {
                        savings::record_file_op(result, p);
                    }
                    AlternativeCost::SymbolFiles(s) => {
                        savings::record_symbol_op(result, s);
                    }
                };
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );

    let outer = "crate::application::middleware::record_operation";
    let inner_file = "crate::application::savings::record_file_op";
    let inner_sym = "crate::application::savings::record_symbol_op";

    let has_file = graph_contains_edge(&graph, outer, inner_file);
    let has_sym = graph_contains_edge(&graph, outer, inner_sym);

    assert!(
        has_file && has_sym,
        "bug2-match — calls inside match-arm bodies not traced.\n\
         record_operation → record_file_op: {has_file}\n\
         record_operation → record_symbol_op: {has_sym}\n\
         record_operation callees: {:?}",
        callees_of(&graph, outer),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 1_tool_router — `#[tool_router]` attribute on impl block hides
// the inner method bodies from the call graph.
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug1_tool_router_attr_on_impl_block_loses_calls() {
    // Verified rlm cause: every #[tool] method inside a
    // `#[tool_router]`-decorated impl block has its body invisible
    // to rustqual's walker.
    //
    // This fixture replicates the rmcp shape as closely as we can
    // without depending on the real rmcp crate. The attributes are
    // unknown to syn (no proc-macro is actually expanded — syn just
    // stores them) so if the test PASSES, the rlm finding can't be
    // explained by attributes alone; the real expansion shape inside
    // rmcp must be different and we need a deeper diagnostic.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            pub struct MyErr;
            impl RlmSession {
                pub fn open(_p: &str) -> Result<Self, MyErr> { todo!() }
            }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            use crate::application::session::{RlmSession, MyErr};

            pub struct RlmServer;
            pub struct Parameters<T>(pub T);
            pub struct SearchParams;
            pub struct CallToolResult;
            pub struct McpError;

            #[tool_router]
            impl RlmServer {
                #[tool]
                pub async fn search(
                    &self,
                    _params: Parameters<SearchParams>,
                ) -> Result<CallToolResult, McpError> {
                    let _session = RlmSession::open("/p");
                    todo!()
                }
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );

    let search = "crate::mcp::server::RlmServer::search";
    let open = "crate::application::session::RlmSession::open";

    let edge = graph_contains_edge(&graph, search, open);

    assert!(
        edge,
        "bug1_tool_router — RlmSession::open is invisible from inside \
         the #[tool_router]-decorated impl block.\n\
         search callees: {:?}",
        callees_of(&graph, search),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 1_private_methods — `async fn` without `pub` modifier inside
// inherent impl block is NOT enumerated as an adapter handler.
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug1_private_async_fn_not_counted_as_adapter_handler() {
    // The real rmcp 0.14 pattern (verified with rlm maintainer
    // 2026-05-13):
    //
    //   #[tool_router(vis = "pub(super)")]
    //   impl RlmServer {
    //       #[tool(description = "...")]
    //       async fn search(&self, ...) -> Result<...> {
    //           // calls into application layer
    //       }
    //   }
    //
    // The `async fn search` has NO `pub` modifier — the proc-macro
    // generates a public dispatch wrapper at expansion time. rustqual
    // reads pre-expansion source, so it sees only the private fn and
    // pub_fns.rs:254 filters it out of `pub_fns_by_layer["mcp"]`.
    // Consequently:
    //   1. mcp has zero handlers reaching RlmSession::open transitively
    //      → "RlmSession::open is not reached from mcp" finding.
    //   2. search isn't enumerated as a handler at all → not flagged
    //      for multi-touchpoint even when it has multiple application
    //      calls.
    //
    // This test runs Check B on a private-method fixture and asserts
    // RlmSession::open is reached from mcp (which only holds if
    // private impl methods are still treated as handler entry points).
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            pub struct MyErr;
            impl RlmSession {
                pub fn open(_p: &str) -> Result<Self, MyErr> { todo!() }
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::RlmSession;
            pub fn cmd_search() {
                let _ = RlmSession::open("/p");
            }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            use crate::application::session::{RlmSession, MyErr};

            #[derive(Clone)]
            pub struct RlmServer {
                pub(super) project_root: std::path::PathBuf,
            }

            pub struct Parameters<T>(pub T);
            pub struct SearchParams;
            pub struct CallToolResult;

            #[tool_router(vis = "pub(super)")]
            impl RlmServer {
                #[tool(description = "search")]
                async fn search(
                    &self,
                    _params: Parameters<SearchParams>,
                ) -> Result<CallToolResult, MyErr> {
                    let _session = RlmSession::open("/p");
                    todo!()
                }
            }
            "#,
        ),
    ]);
    // After the Bug 1 fix: configuring `promoted_attributes = ["tool"]`
    // lifts the private `async fn search` onto the mcp-handler surface,
    // so its `RlmSession::open` call shows up in mcp's coverage and the
    // missing-adapter finding clears. (Without the config, the finding
    // remains; that asymmetry is what the discoverable-hint task #17
    // exposes to the author.)
    let mut cp = rlm_cfg_for_eval();
    cp.promoted_attributes.insert("tool".to_string());
    let findings = run_check_b(&ws, &rlm_layers_for_eval(), &cp, &empty_cfg_test());

    let open = "crate::application::session::RlmSession::open";
    let missing = missing_adapters_for(&findings, open);

    assert!(
        missing
            .as_ref()
            .is_none_or(|m| !m.contains(&"mcp".to_string())),
        "bug1_private_async_fn — even with promoted_attributes=[\"tool\"], \
         RlmSession::open is still flagged as not reached from mcp. \
         The promotion in pub_fns.rs::visit_impl_item_fn isn't picking \
         up the #[tool] attribute on the private async fn.\n\
         missing adapters for RlmSession::open: {missing:?}\n\
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

// ═══════════════════════════════════════════════════════════════════
// Bug 2_pub_use — caller imports callee via a parent-module `pub use`
// re-export → rustqual loses the trace.
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug2_pub_use_reexport_loses_callee_edge() {
    // Verified rlm cause: callers import `middleware::record_operation`
    // (re-exported via `pub use savings_recorder::record_operation;`).
    // rustqual canonicalises the call to `middleware::record_operation`
    // — there's no fn definition at that path → edge dropped.
    //
    // Same fn, same module hierarchy, same `pub use`. Only the import
    // path on the caller decides whether the edge survives.
    let ws = build_workspace(&[
        (
            "src/application/middleware/mod.rs",
            r#"
            pub mod savings_recorder;
            pub use savings_recorder::record_operation;
            "#,
        ),
        (
            "src/application/middleware/savings_recorder.rs",
            r#"
            pub fn record_operation() {}
            "#,
        ),
        (
            "src/application/session.rs",
            r#"
            use crate::application::middleware;
            pub struct Session;
            impl Session {
                pub fn search(&self) {
                    middleware::record_operation();
                }
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(
        &ws,
        &rlm_layers_for_eval(),
        &empty_cfg_test(),
        &HashSet::new(),
    );

    let search = "crate::application::session::Session::search";
    let real_target = "crate::application::middleware::savings_recorder::record_operation";

    let edge = graph_contains_edge(&graph, search, real_target);

    assert!(
        edge,
        "bug2_pub_use — call site `middleware::record_operation()` \
         not resolved through the `pub use` re-export to the real \
         definition `{real_target}`.\n\
         search callees: {:?}",
        callees_of(&graph, search),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 5_unknown_dim — `// qual:allow(srp_params)` silently falls back
// to the global-suppress form because `srp_params` isn't a recognized
// Dimension and the parser strips it instead of rejecting.
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug5_qual_allow_must_not_global_suppress() {
    use crate::adapters::suppression::qual_allow::parse_suppression;

    // User decision (2026-05-13): global-suppress must not exist as
    // a feature at all. Two annotation forms must both be IGNORED
    // (treated as if the comment isn't there):
    //
    //   (a) `// qual:allow`           — no dimension specified
    //   (b) `// qual:allow(srp_params)` — unknown dimension name
    //
    // Both currently parse to `Suppression { dimensions: vec![], … }`,
    // which downstream treats as "suppress every dimension on this
    // function" — silently hiding architecture, IOSP, complexity,
    // every finding category. That's the verified rlm symptom: the
    // `srp_params` typo on `cmd_replace` silenced the call_parity
    // multi-touchpoint finding.
    //
    // After the fix, both forms must EITHER return `None` (parser
    // rejection) OR return a Suppression with a non-empty dimensions
    // list — never an empty-dimensions Suppression.

    let cases = [
        ("// qual:allow", "bare allow (no parens)"),
        ("// qual:allow(srp_params)", "unknown dim name"),
        (
            "// qual:allow(srp_params) reason: \"typo\"",
            "unknown dim with reason",
        ),
        ("// qual:allow()", "empty parens"),
    ];

    let mut failures: Vec<String> = Vec::new();
    for (input, label) in &cases {
        match parse_suppression(1, input) {
            None => {
                // Acceptable: parser rejected.
            }
            Some(s) if s.dimensions.is_empty() => {
                failures.push(format!(
                    "{label}: `{input}` → empty-dimensions Suppression \
                     (global suppress)"
                ));
            }
            Some(_) => {
                // Acceptable: parser returned a non-empty dim list.
            }
        }
    }

    assert!(
        failures.is_empty(),
        "bug5 — global suppress must not be reachable via empty / \
         unknown-dimension qual:allow forms. Offending cases:\n  - {}",
        failures.join("\n  - "),
    );
}

// ═══════════════════════════════════════════════════════════════════
// Bug 3 — cascade clearance via build_adapter_reachable_targets
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug3_cascade_via_reachable_targets_after_bug4_fix() {
    // Setup: an adapter reaches a generic runner that dispatches via
    // `Q::execute(...)` to a trait impl. The impl's body calls another
    // application helper. Once Bug 4 is fixed, that helper should
    // automatically clear from Check B's missing-adapter findings via
    // `build_adapter_reachable_targets` (forward BFS through
    // target-internal edges from the boundary touchpoints).
    //
    // Under HEAD this test FAILS (because the trait-anchor edge is
    // missing → BFS never reaches the impl method → its callee is
    // flagged as orphan). It documents Bug 3's *dependency* on Bug 4
    // — green-on-fix without separate code change.
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
        &rlm_layers_for_eval(),
        &rlm_cfg_for_eval(),
        &empty_cfg_test(),
    );

    let collect_canonical = "crate::application::symbol::collect_callees";
    let missing = missing_adapters_for(&findings, collect_canonical);

    assert!(
        missing.is_none(),
        "bug3 — `collect_callees` flagged as not-reached even though \
         it's called by an impl method that an adapter reaches via \
         the generic runner.\n\
         missing adapters for collect_callees: {:?}\n\
         all findings: {:?}",
        missing,
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

// ═══════════════════════════════════════════════════════════════════
// Bug 1 (Hint) — `CallParityMissingAdapter` carries a hint that points
// at private attributed fns which would resolve the finding if promoted.
// ═══════════════════════════════════════════════════════════════════

#[test]
fn bug17_hint_appears_when_private_attributed_fn_reaches_target() {
    // Positive case: missing adapter mcp has a private `#[tool]` fn
    // that transitively reaches the unreached target. The hint must
    // name the file, the fn, AND the attribute so the author can
    // immediately add `promoted_attributes = ["tool"]`.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            pub struct MyErr;
            impl RlmSession {
                pub fn open(_p: &str) -> Result<Self, MyErr> { todo!() }
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::RlmSession;
            pub fn cmd_search() {
                let _ = RlmSession::open("/p");
            }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            use crate::application::session::{RlmSession, MyErr};
            pub struct RlmServer;
            impl RlmServer {
                #[tool(description = "search")]
                async fn search(&self) -> Result<(), MyErr> {
                    let _ = RlmSession::open("/p");
                    Ok(())
                }
            }
            "#,
        ),
    ]);
    // `promoted_attributes` empty: the private fn isn't promoted →
    // RlmSession::open is missing from mcp's coverage → the hint
    // must point at the candidate so the author knows the fix.
    let cp = rlm_cfg_for_eval();
    let findings = run_check_b(&ws, &rlm_layers_for_eval(), &cp, &empty_cfg_test());

    let open = "crate::application::session::RlmSession::open";
    let hint = hint_for(&findings, open).expect("expected hint on Bug 17 finding");
    assert!(
        hint.contains("search") && hint.contains("tool") && hint.contains("mcp"),
        "hint must name the candidate fn `search`, its attribute `tool`, and the \
         adapter `mcp`. Got:\n{hint}"
    );
    assert!(
        hint.contains("promoted_attributes"),
        "hint must point the author at the `promoted_attributes` config knob. Got:\n{hint}"
    );
}

#[test]
fn bug17_no_hint_when_no_candidate() {
    // Negative: missing adapter mcp has NO private fn carrying any
    // attribute that reaches the target. The finding still fires but
    // hint is None — nothing to suggest.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            impl RlmSession {
                pub fn open(_p: &str) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::RlmSession;
            pub fn cmd_search() { RlmSession::open("/p"); }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            pub fn handle_other() {}
            "#,
        ),
    ]);
    let cp = rlm_cfg_for_eval();
    let findings = run_check_b(&ws, &rlm_layers_for_eval(), &cp, &empty_cfg_test());

    let open = "crate::application::session::RlmSession::open";
    let missing =
        missing_adapters_for(&findings, open).expect("expected open to be missing from mcp");
    assert!(missing.contains(&"mcp".to_string()));
    assert!(
        hint_for(&findings, open).is_none(),
        "no private attributed candidate in mcp → no hint expected"
    );
}

#[test]
fn bug17_no_hint_when_attribute_is_stdlib() {
    // Negative: private fn carries only stdlib attributes (`#[allow]`,
    // `#[deprecated]`, etc.). These are noise, not framework-handler
    // markers, so they must not trigger the hint.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            impl RlmSession {
                pub fn open(_p: &str) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::RlmSession;
            pub fn cmd_search() { RlmSession::open("/p"); }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            use crate::application::session::RlmSession;
            pub struct RlmServer;
            impl RlmServer {
                #[allow(dead_code)]
                fn search_internal(&self) {
                    RlmSession::open("/p");
                }
            }
            "#,
        ),
    ]);
    let cp = rlm_cfg_for_eval();
    let findings = run_check_b(&ws, &rlm_layers_for_eval(), &cp, &empty_cfg_test());

    let open = "crate::application::session::RlmSession::open";
    assert!(
        hint_for(&findings, open).is_none(),
        "stdlib attribute (#[allow]) must not qualify the fn as a promotion candidate"
    );
}

#[test]
fn bug17_no_hint_when_private_fn_does_not_reach_target() {
    // Negative: private + attributed fn exists in the missing adapter
    // BUT its body doesn't reach the unreached target. Promoting it
    // wouldn't fix the finding, so no hint.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            impl RlmSession {
                pub fn open(_p: &str) {}
                pub fn other(&self) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::RlmSession;
            pub fn cmd_search() { RlmSession::open("/p"); }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            pub struct RlmServer;
            impl RlmServer {
                #[tool(description = "unrelated")]
                async fn unrelated(&self) {
                    // Doesn't call RlmSession::open — calls nothing.
                }
            }
            "#,
        ),
    ]);
    let cp = rlm_cfg_for_eval();
    let findings = run_check_b(&ws, &rlm_layers_for_eval(), &cp, &empty_cfg_test());

    let open = "crate::application::session::RlmSession::open";
    assert!(
        hint_for(&findings, open).is_none(),
        "the #[tool] fn `unrelated` doesn't reach RlmSession::open → no hint expected"
    );
}

#[test]
fn bug17_hint_excludes_already_covering_adapter() {
    // Mismatch case: cli reaches target, mcp doesn't.
    // missing_adapters=[mcp]. The hint must mention only mcp's
    // candidate (if any), never cli's — cli already covers the
    // target, so promoting a cli candidate is irrelevant.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            impl RlmSession {
                pub fn stats(&self) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::RlmSession;
            pub struct CliCmds;
            impl CliCmds {
                // cli has a private #[tool] fn for some reason; it
                // reaches stats. But cli already covers stats via
                // pub fn — so this candidate must NOT show up in the
                // hint.
                #[tool(description = "diagnostic")]
                fn diagnostic_stats(s: &RlmSession) { s.stats(); }
            }
            pub fn cmd_stats(s: &RlmSession) { s.stats(); }
            "#,
        ),
        (
            "src/mcp/server.rs",
            r#"
            pub fn handle_other() {}
            "#,
        ),
    ]);
    let cp = rlm_cfg_for_eval();
    let findings = run_check_b(&ws, &rlm_layers_for_eval(), &cp, &empty_cfg_test());

    let stats = "crate::application::session::RlmSession::stats";
    let missing = missing_adapters_for(&findings, stats).expect("stats must be missing from mcp");
    assert_eq!(missing, vec!["mcp".to_string()]);
    // cli has a candidate but cli isn't a missing adapter → no hint.
    // (mcp has no candidate either, so hint should be None.)
    assert!(
        hint_for(&findings, stats).is_none(),
        "candidate in cli (already covering) must not surface in the hint when only mcp is missing"
    );
}
