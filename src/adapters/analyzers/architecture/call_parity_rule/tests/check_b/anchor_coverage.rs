use super::*;

// Trait in `ports`, impl(s) in `application` (target); CLI + MCP may dispatch
// via `dyn Handler.handle()`, call the concrete impl-method directly, or not
// reach the capability at all. `target` is the canonical we look for (the
// trait-method anchor or a concrete impl-method); `present` is whether a
// missing-adapter finding for it should appear.
// (label, files, exclude_glob, target, present)
const ANCHOR_COVERAGE_CASES: &[AnchorCase] = &[
    (
        "anchor dispatched by all adapters → silent",
        &[
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
        ],
        &[],
        "crate::ports::handler::Handler::handle",
        false,
    ),
    (
        "concrete impl-method silent when only its anchor is dispatched",
        &[
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
        ],
        &[],
        "crate::application::logging::LoggingHandler::handle",
        false,
    ),
    (
        "inherent method collides with inherited default → still flagged",
        &[
            (
                "src/ports/handler.rs",
                "pub trait Handler { fn handle(&self) {} }",
            ),
            (
                "src/application/logging.rs",
                r#"
                    use crate::ports::handler::Handler;
                    pub struct AppHandler;
                    impl Handler for AppHandler {}
                    impl AppHandler { pub fn handle(&self) {} }
                    "#,
            ),
            ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
            ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
        ],
        &[],
        "crate::application::logging::AppHandler::handle",
        true,
    ),
    (
        "anchor silent when all adapters cover via inherited-default concrete",
        &[
            (
                "src/ports/handler.rs",
                "pub trait Handler { fn handle(&self) {} }",
            ),
            (
                "src/application/logging.rs",
                r#"
                    use crate::ports::handler::Handler;
                    pub struct AppHandler;
                    impl Handler for AppHandler {}
                    "#,
            ),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::logging::AppHandler;
                    pub fn cmd_log() { AppHandler::handle(&AppHandler); }
                    "#,
            ),
            (
                "src/mcp/handlers.rs",
                r#"
                    use crate::application::logging::AppHandler;
                    pub fn mcp_log() { AppHandler::handle(&AppHandler); }
                    "#,
            ),
        ],
        &[],
        "crate::ports::handler::Handler::handle",
        false,
    ),
    (
        "anchor finding silenced via impl-path exclude glob",
        &[
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
        ],
        &["application::admin::*"],
        "crate::ports::handler::Handler::handle",
        false,
    ),
    (
        "anchor silent when all adapters cover via direct concrete",
        &[
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
        ],
        &[],
        "crate::ports::handler::Handler::handle",
        false,
    ),
    (
        "anchor reached by no adapter → flagged orphan",
        &[
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
            ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
            ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
        ],
        &[],
        "crate::ports::orphan::Orphan::handle",
        true,
    ),
    (
        "anchor reached transitively via an adapter-touched target fn → silent",
        &[
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
        ],
        &[],
        "crate::ports::handler::Handler::handle",
        false,
    ),
];

#[test]
fn check_b_trait_anchor_coverage() {
    for (label, files, exclude, target, present) in ANCHOR_COVERAGE_CASES {
        let mut cp = ports_cp();
        cp.exclude_targets = globset(exclude);
        let ws = build_workspace(files);
        let findings = run_check_b(&ws, &ports_app_cli_mcp(), &cp, &empty_cfg_test());
        let pairs = missing_pairs(&findings);
        let found = pairs.iter().any(|(t, _)| t == *target);
        assert_eq!(found, *present, "case {label}: pairs={pairs:?}");
    }
}
