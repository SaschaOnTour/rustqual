use super::*;

// The exact target-layer canonical set an adapter handler reaches before the
// boundary BFS stops: direct calls, adapter-helper hops, branches/loops
// collapsing to distinct targets, the boundary cutoff (target callees excluded),
// call_depth limits, trait-dispatch collapsing to one anchor, and peer-adapter
// walks producing nothing. (label, files, depth, handler, expected)
const TOUCHPOINT_SET_CASES: &[TouchpointCase] = &[
    (
        "direct call to a target fn",
        &[
            ("src/application/session.rs", "pub fn search() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::session::search;
                    pub fn cmd_search() { search(); }
                    "#,
            ),
        ],
        3,
        "cmd_search",
        &["crate::application::session::search"],
    ),
    (
        "adapter helper traversed before the boundary",
        &[
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
        ],
        3,
        "cmd_search",
        &["crate::application::session::search"],
    ),
    (
        "no delegation: handler stays in the adapter layer",
        &[
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
        ],
        3,
        "cmd_local_only",
        &[],
    ),
    (
        "branch with two distinct target fns",
        &[
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
        ],
        3,
        "cmd_branch",
        &[
            "crate::application::session::foo",
            "crate::application::session::bar",
        ],
    ),
    (
        "loop: same target at many call sites collapses to one",
        &[
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
        ],
        3,
        "cmd_batch",
        &["crate::application::session::process"],
    ),
    (
        // boundary semantic: application-internal `record_operation`
        // reached past `search` must NOT appear in the set
        "stop at the target boundary",
        &[
            (
                "src/application/session.rs",
                r#"
                    use crate::application::middleware::record_operation;
                    pub fn search() { record_operation(); }
                    "#,
            ),
            (
                "src/application/middleware.rs",
                "pub fn record_operation() {}",
            ),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::session::search;
                    pub fn cmd_search() { search(); }
                    "#,
            ),
        ],
        3,
        "cmd_search",
        &["crate::application::session::search"],
    ),
    (
        // cmd → h1 → h2 → h3 → h4 → search (5 hops); depth 3 stops short
        "call_depth exceeded returns the empty set",
        &[
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
        ],
        3,
        "cmd_deep",
        &[],
    ),
    (
        // cmd → h1 → h2 → search (3 hops); depth 3 just reaches it
        "call_depth just inside the limit reaches the target",
        &[
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
        ],
        3,
        "cmd_depth3",
        &["crate::application::session::search"],
    ),
    (
        // three overriding impls collapse to ONE `<Trait>::<method>` anchor
        "trait dispatch collapses to a single anchor",
        &[
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
        ],
        3,
        "cmd_dispatch",
        &["crate::application::handler::Handler::handle"],
    ),
    (
        // anchor declared in a peer-adapter (mcp) layer must not be a
        // target boundary, even with a target-layer overriding impl
        "anchor declared in a peer-adapter layer is skipped",
        &[
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
        ],
        3,
        "cmd_via_dyn_peer",
        &[],
    ),
    (
        // CLI → MCP handler → application: the CLI walk must not inherit
        // MCP's touchpoint by traversing a peer adapter
        "peer-adapter traversal is blocked",
        &[
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
        ],
        3,
        "cmd_via_mcp",
        &[],
    ),
];

#[test]
fn touchpoint_sets() {
    for (label, files, depth, handler, expected) in TOUCHPOINT_SET_CASES {
        let touchpoints = tp_3l(files, *depth, handler);
        let expected_set: HashSet<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            touchpoints, expected_set,
            "case {label}: actual={touchpoints:?} expected={expected_set:?}"
        );
    }
}
