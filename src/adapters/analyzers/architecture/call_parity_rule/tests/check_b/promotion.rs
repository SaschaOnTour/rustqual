use super::*;

// ─────────────────────────────────────────────────────────────────────
// Cascade clearance — when an adapter reaches a generic dispatcher
// that resolves to a trait-anchor edge, downstream callees of the
// impl method must be cleared from missing-adapter findings via the
// forward BFS through target-internal edges.
// ─────────────────────────────────────────────────────────────────────

/// 4-file workspace for the cascade test: a generic `run<Q: SymbolQuery>` that
/// dispatches via `Q::execute` to a `DepsQuery` impl which calls
/// `collect_callees`, reached from both the CLI and MCP adapters.
fn cascade_workspace() -> Vec<(&'static str, &'static str)> {
    vec![
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
    ]
}

#[test]
fn check_b_cascade_clears_when_target_reached_via_generic_trait_dispatch() {
    // An adapter reaches a generic runner that dispatches via
    // `Q::execute(...)` to a trait impl. The impl body calls another
    // application helper. With the trait-anchor edge in place, the
    // helper must NOT show up as missing — the BFS from the boundary
    // touchpoints follows the anchor to the impl, then onwards.
    let findings = run_b_three(&cascade_workspace());

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

/// 3-file workspace for the promoted-attribute test: a `Session::open` target,
/// a CLI handler reaching it, and an MCP `Server` whose PRIVATE `#[tool] async fn
/// search` reaches it (visible to coverage only once `#[tool]` is promoted).
fn promoted_attribute_workspace() -> Vec<(&'static str, &'static str)> {
    vec![
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
    ]
}

#[test]
fn check_b_promoted_attribute_lifts_private_fn_onto_handler_surface() {
    // Pattern: `async fn search` without a `pub` modifier inside an
    // inherent impl block — a proc-macro is expected to generate the
    // public dispatch wrapper at expansion time. Without
    // `promoted_attributes` configured, the private fn is filtered
    // out of the adapter's handler set and the call is invisible.
    // With `promoted_attributes = ["tool"]`, the fn is promoted onto
    // the surface and its body's calls satisfy coverage.
    let ws = build_workspace(&promoted_attribute_workspace());
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
