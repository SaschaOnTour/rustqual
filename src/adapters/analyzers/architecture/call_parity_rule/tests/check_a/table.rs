use super::*;

// ── Basic direct / inline cases ───────────────────────────────

// Check A flags an adapter pub-fn when it does NOT reach a target-layer fn
// within `call_depth` hops (own-layer/peer-adapter hops don't count; method
// calls on unknown types and cross-adapter calls don't delegate).
// (label, files, depth, fn_name, flagged)
const DELEGATION_CASES: &[DelegationCase] = &[
    (
        "direct delegation to target passes",
        &[
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
        ],
        3,
        "cmd_stats",
        false,
    ),
    (
        "inline-only adapter fn is flagged",
        &[
            ("src/application/stats.rs", "pub fn get_stats() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    pub fn cmd_stats() {
                        let _ = 42;
                    }
                    "#,
            ),
        ],
        3,
        "cmd_stats",
        true,
    ),
    (
        "transitive delegation via a same-layer helper passes",
        &[
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
        ],
        3,
        "cmd_stats",
        false,
    ),
    (
        // cmd_stats → h1 → h2 → h3 → h4 → get_stats (5 hops); depth 3
        // explores only 3 edges deep → target never reached.
        "delegation chain exceeding call_depth is flagged",
        &[
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
        ],
        3,
        "cmd_stats",
        true,
    ),
    (
        // call_depth=1: only direct calls count, helper isn't target → fail
        "call_depth=1 counts only direct calls",
        &[
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
        ],
        1,
        "cmd_stats",
        true,
    ),
    (
        // `disp.run()` on an unknown type stays `<method>:run` = no delegation
        "method call on unknown type does not count",
        &[
            ("src/application/dispatch.rs", "pub fn run_it() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    pub fn cmd_stats(disp: UnknownType) {
                        disp.run();
                    }
                    "#,
            ),
        ],
        3,
        "cmd_stats",
        true,
    ),
    (
        // CLI → MCP (peer adapter, not target) earns no delegation credit
        "cross-adapter call at depth 1 does not count",
        &[
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
        ],
        1,
        "cmd_stats",
        true,
    ),
    (
        // even at deeper depth, a peer-adapter walk must not inherit
        // MCP's application touchpoints
        "cross-adapter call blocked even at deeper depth",
        &[
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
        ],
        2,
        "cmd_stats",
        true,
    ),
    (
        // unparseable `impl dyn Debug` self-type must be skipped, leaving
        // the free fn `search` intact as cmd_x's delegation target
        "unparseable impl self-type does not collapse with free fns",
        &[
            ("src/application/stats.rs", "pub fn get_stats() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::stats::get_stats;
                    pub fn search() { get_stats(); }
                    impl dyn std::fmt::Debug {
                        pub fn search(&self) {}
                    }
                    pub fn cmd_x() { search(); }
                    "#,
            ),
        ],
        3,
        "cmd_x",
        false,
    ),
    (
        // convergent fan-out must still terminate and resolve delegation
        "convergent graph does not double-enqueue",
        &[
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
        ],
        3,
        "cmd_x",
        false,
    ),
    (
        // a `#[deprecated]` non-delegating adapter fn is excluded
        "deprecated handler is skipped",
        &[
            ("src/application/stats.rs", "pub fn get_stats() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    #[deprecated]
                    pub fn cmd_old() { let _ = 42; }
                    "#,
            ),
        ],
        3,
        "cmd_old",
        false,
    ),
];

#[test]
fn adapter_delegation_flagging() {
    for (label, files, depth, fn_name, flagged) in DELEGATION_CASES {
        let names = delegation_names(files, *depth);
        assert_eq!(
            names.contains(&fn_name.to_string()),
            *flagged,
            "case {label}: flagged fns = {names:?}"
        );
    }
}
