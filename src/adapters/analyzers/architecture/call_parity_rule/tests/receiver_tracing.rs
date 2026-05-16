//! Workspace-level tests for method-call tracing through receivers,
//! locally-bound bindings, parameter-bound generics, async fn bodies,
//! generic-fn path calls, and inline attribute-decorated impl blocks.
//!
//! Each test builds a multi-file workspace via `build_workspace`,
//! produces the full call graph through `build_graph_only`, and
//! asserts the expected `crate::…::Type::method` (or free-fn) edge.
//! Coverage targets the inference paths that turn a syntactic call
//! site into a canonical edge: `self.helper()` chains, locally-bound
//! `let x = open()?; x.method()`, parameter-typed receivers, and
//! `path::call()` inside generic / `match` / async bodies.

use super::support::{
    build_graph_only, build_workspace, callees_of, empty_cfg_test, graph_contains_edge, three_layer,
};
use std::collections::HashSet;

// ─────────────────────────────────────────────────────────────────────
// Self-receiver chains: `self.helper()` reaches an associated fn on
// a different type two hops away.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn self_method_chain_traces_through_helper_to_associated_fn_on_other_type() {
    // Shape:
    //   impl Server {
    //       fn ensure_session(&self) -> Session { Session::open("/p") }
    //   }
    //   impl Server {
    //       pub fn search(&self) { self.ensure_session(); }
    //   }
    //
    // Asserts both edges exist:
    //   Server::search → Server::ensure_session
    //   Server::ensure_session → Session::open
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
            "src/mcp/server.rs",
            r#"
            use crate::application::session::{Session, MyErr};

            pub struct Server;

            impl Server {
                pub(crate) fn ensure_session(&self) -> Result<Session, MyErr> {
                    Session::open("/p")
                }
            }

            impl Server {
                pub fn search(&self) -> Result<(), MyErr> {
                    self.ensure_session()?;
                    Ok(())
                }
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let search = "crate::mcp::server::Server::search";
    let ensure = "crate::mcp::server::Server::ensure_session";
    let open = "crate::application::session::Session::open";

    let edge1 = graph_contains_edge(&graph, search, ensure);
    let edge2 = graph_contains_edge(&graph, ensure, open);

    assert!(
        edge1 && edge2,
        "self-method chain `search → ensure_session → Session::open` broken.\n\
         search → ensure_session: {edge1}\n\
         ensure_session → Session::open: {edge2}\n\
         search callees: {:?}\n\
         ensure_session callees: {:?}",
        callees_of(&graph, search),
        callees_of(&graph, ensure),
    );
}

#[test]
fn async_self_method_chain_traces_through_helper_to_associated_fn() {
    // Same chain as the sync variant, but `pub async fn search` —
    // async fn lowering must not break the edge.
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
            "src/mcp/server.rs",
            r#"
            use crate::application::session::{Session, MyErr};

            pub struct Server;

            impl Server {
                pub(crate) fn ensure_session(&self) -> Result<Session, MyErr> {
                    Session::open("/p")
                }
            }

            impl Server {
                pub async fn search(&self) -> Result<(), MyErr> {
                    self.ensure_session()?;
                    Ok(())
                }
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let search = "crate::mcp::server::Server::search";
    let ensure = "crate::mcp::server::Server::ensure_session";
    let open = "crate::application::session::Session::open";

    let edge1 = graph_contains_edge(&graph, search, ensure);
    let edge2 = graph_contains_edge(&graph, ensure, open);

    assert!(
        edge1 && edge2,
        "async self-method chain broken.\n\
         search → ensure_session: {edge1}\n\
         ensure_session → Session::open: {edge2}\n\
         search callees: {:?}\n\
         ensure_session callees: {:?}",
        callees_of(&graph, search),
        callees_of(&graph, ensure),
    );
}

// ─────────────────────────────────────────────────────────────────────
// Locally-bound and parameter-bound receivers: `let s = open()?; s.m()`
// and `fn h(s: &Session) { s.m() }` must both produce method edges.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn locally_bound_let_receiver_traces_multiple_method_calls() {
    // `let session = open_session_in_cwd()?;` followed by branches
    // calling `session.replace_preview()` and `session.replace_apply()`
    // must produce TWO edges from the enclosing fn.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            pub struct MyErr;
            impl Session {
                pub fn replace_preview(&self) {}
                pub fn replace_apply(&self) {}
            }
            "#,
        ),
        (
            "src/cli/helpers.rs",
            r#"
            use crate::application::session::{Session, MyErr};
            pub fn open_session_in_cwd() -> Result<Session, MyErr> { todo!() }
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
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let cmd = "crate::cli::handlers::cmd_replace";
    let preview = "crate::application::session::Session::replace_preview";
    let apply = "crate::application::session::Session::replace_apply";

    let has_preview = graph_contains_edge(&graph, cmd, preview);
    let has_apply = graph_contains_edge(&graph, cmd, apply);

    assert!(
        has_preview && has_apply,
        "locally-bound `session.X()` calls not traced.\n\
         cmd_replace → replace_preview: {has_preview}\n\
         cmd_replace → replace_apply:   {has_apply}\n\
         cmd_replace callees: {:?}",
        callees_of(&graph, cmd),
    );
}

#[test]
fn parameter_bound_receiver_traces_multiple_method_calls() {
    // `pub fn handle_replace(session: &Session, preview: bool)` —
    // calls inside both `if` branches must produce edges.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            impl Session {
                pub fn replace_preview(&self) {}
                pub fn replace_apply(&self) {}
            }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::Session;

            pub fn handle_replace(session: &Session, preview: bool) {
                if preview {
                    session.replace_preview();
                } else {
                    session.replace_apply();
                }
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let h = "crate::mcp::handlers::handle_replace";
    let preview = "crate::application::session::Session::replace_preview";
    let apply = "crate::application::session::Session::replace_apply";

    let has_preview = graph_contains_edge(&graph, h, preview);
    let has_apply = graph_contains_edge(&graph, h, apply);

    assert!(
        has_preview && has_apply,
        "parameter-bound `session.X()` calls not traced.\n\
         handle_replace → replace_preview: {has_preview}\n\
         handle_replace → replace_apply:   {has_apply}\n\
         handle_replace callees: {:?}",
        callees_of(&graph, h),
    );
}

// ─────────────────────────────────────────────────────────────────────
// Generic-fn bodies: a path call `savings::record_file_op(result, p)`
// inside a generic fn body must emit a path-call edge regardless of
// the surrounding generics.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn generic_fn_body_traces_path_call_to_generic_target() {
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
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let outer = "crate::application::middleware::record_operation";
    let inner = "crate::application::savings::record_file_op";

    assert!(
        graph_contains_edge(&graph, outer, inner),
        "direct path-call edge from generic `record_operation` to \
         generic `record_file_op` missing.\n\
         record_operation callees: {:?}",
        callees_of(&graph, outer),
    );
}

#[test]
fn path_call_inside_generic_match_arm_traces_edge() {
    // Outer call lives inside a `match` arm body — must still produce
    // the edge for each arm.
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
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let outer = "crate::application::middleware::record_operation";
    let inner_file = "crate::application::savings::record_file_op";
    let inner_sym = "crate::application::savings::record_symbol_op";

    let has_file = graph_contains_edge(&graph, outer, inner_file);
    let has_sym = graph_contains_edge(&graph, outer, inner_sym);

    assert!(
        has_file && has_sym,
        "calls inside match-arm bodies not traced.\n\
         record_operation → record_file_op: {has_file}\n\
         record_operation → record_symbol_op: {has_sym}\n\
         record_operation callees: {:?}",
        callees_of(&graph, outer),
    );
}

// ─────────────────────────────────────────────────────────────────────
// Inline impl-block attributes (`#[tool_router]` / `#[tool]`) must not
// hide the inner method bodies from the call graph. syn stores unknown
// attrs verbatim — the walker must descend into the impl regardless.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn unknown_attribute_on_impl_block_does_not_hide_inner_calls() {
    // Method bodies inside an impl block decorated with a non-cfg
    // unknown attribute must still contribute edges to the call graph.
    // The attributes here are unknown to syn (no proc-macro expansion
    // happens — syn just records them) so this test verifies the
    // visitor doesn't skip attribute-decorated impl blocks.
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
            "src/mcp/server.rs",
            r#"
            use crate::application::session::{Session, MyErr};

            pub struct Server;
            pub struct Parameters<T>(pub T);
            pub struct SearchParams;
            pub struct CallToolResult;
            pub struct McpError;

            #[tool_router]
            impl Server {
                #[tool]
                pub async fn search(
                    &self,
                    _params: Parameters<SearchParams>,
                ) -> Result<CallToolResult, McpError> {
                    let _session = Session::open("/p");
                    todo!()
                }
            }
            "#,
        ),
    ]);
    let graph = build_graph_only(&ws, &three_layer(), &empty_cfg_test(), &HashSet::new());

    let search = "crate::mcp::server::Server::search";
    let open = "crate::application::session::Session::open";

    let edge = graph_contains_edge(&graph, search, open);

    assert!(
        edge,
        "Session::open is invisible from inside the \
         #[tool_router]-decorated impl block.\n\
         search callees: {:?}",
        callees_of(&graph, search),
    );
}
