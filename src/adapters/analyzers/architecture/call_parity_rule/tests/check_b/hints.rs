use super::*;

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

// The missing-adapter finding fires (target is missing from mcp), but no hint
// is offered because no promotable private candidate in mcp would actually close
// the gap: there's no attributed candidate, only a stdlib attribute, the
// candidate body doesn't reach the target, the only candidate lives in an
// already-covering adapter, or its path to the target exceeds call_depth /
// leaves through a peer adapter. (label, files, depth, target)
const NO_HINT_CASES: &[NoHintCase] = &[
    (
        "no private attributed fn in the missing adapter",
        &[
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
            ("src/mcp/server.rs", "pub fn handle_other() {}"),
        ],
        3,
        "crate::application::session::Session::open",
    ),
    (
        "candidate carries only a stdlib attribute (#[allow])",
        &[
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
        ],
        3,
        "crate::application::session::Session::open",
    ),
    (
        "attributed candidate body does not reach the target",
        &[
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
        ],
        3,
        "crate::application::session::Session::open",
    ),
    (
        "only candidate lives in an adapter already covering the target",
        &[
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
            ("src/mcp/server.rs", "pub fn handle_other() {}"),
        ],
        3,
        "crate::application::session::Session::stats",
    ),
    (
        "candidate path to target exceeds call_depth",
        &[
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
        ],
        1,
        "crate::application::session::Session::open",
    ),
    (
        "candidate reaches the target only via a peer adapter",
        &[
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
        ],
        3,
        "crate::application::session::Session::open",
    ),
];

#[test]
fn check_b_no_hint_when_promotion_would_not_resolve_the_finding() {
    for (label, files, depth, target) in NO_HINT_CASES {
        let mut cp = cli_mcp_config_full();
        cp.call_depth = *depth;
        let findings = run_check_b(
            &build_workspace(files),
            &three_layer(),
            &cp,
            &empty_cfg_test(),
        );
        let missing = missing_adapters_for(&findings, target)
            .unwrap_or_else(|| panic!("case {label}: target must be missing from mcp"));
        assert_eq!(missing, vec!["mcp".to_string()], "case {label}");
        assert!(
            hint_for(&findings, target).is_none(),
            "case {label}: expected no hint, got {:?}",
            hint_for(&findings, target)
        );
    }
}
