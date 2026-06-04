use super::*;

#[test]
fn touchpoints_trait_anchor_recognized_when_trait_lives_in_ports_layer() {
    // Hexagonal/Ports&Adapters case: trait declared in `ports`, impls in
    // `application` (target). Dispatch emits the anchor `ports::Handler::
    // handle`; the walker must still register it as a target boundary
    // because its impls reach the target — otherwise Check A would falsely
    // fire "no delegation" for a CLI command that crosses via trait dispatch.
    let touchpoints = tp_ports(
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
        ],
        "cmd_dispatch",
    );
    assert_set(touchpoints, &["crate::ports::handler::Handler::handle"]);
}

// Whether a specific canonical is registered as a touchpoint under the
// hexagonal layout: a phantom inherited-default concrete canonical is rejected
// (no real fn node); the same call routes to the trait anchor when the anchor is
// a valid target capability; an ambiguous multi-trait default leaves the call
// unresolved (non-deterministic otherwise); and a real concrete target pub-fn IS
// a boundary. (label, files, handler, anchor, present)
const TOUCHPOINT_ANCHOR_CASES: &[ContainsCase] = &[
    (
        // empty `impl Handler for AppHandler {}`; default body lives on
        // the ports trait, so the concrete canonical is phantom
        "phantom inherited-default concrete canonical is rejected",
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
        ],
        "cmd_log",
        "crate::application::logging::AppHandler::handle",
        false,
    ),
    (
        // target-layer trait with default body: the anchor IS a valid
        // target capability, so the concrete call routes to it
        "inherited-default concrete call routes to the target anchor",
        &[
            (
                "src/application/handler.rs",
                "pub trait Handler { fn handle(&self) {} }",
            ),
            (
                "src/application/logging.rs",
                r#"
                    use crate::application::handler::Handler;
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
        ],
        "cmd_log",
        "crate::application::handler::Handler::handle",
        true,
    ),
    (
        // AppHandler implements two traits with the same default method
        // name; rewrite must not pick one (HashMap-order dependent) →
        // neither anchor is registered
        "ambiguous multi-trait default leaves Greeting unresolved",
        &[
            (
                "src/application/handler.rs",
                r#"
                    pub trait Greeting { fn handle(&self) {} }
                    pub trait Logging { fn handle(&self) {} }
                    "#,
            ),
            (
                "src/application/logging.rs",
                r#"
                    use crate::application::handler::{Greeting, Logging};
                    pub struct AppHandler;
                    impl Greeting for AppHandler {}
                    impl Logging for AppHandler {}
                    "#,
            ),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::logging::AppHandler;
                    pub fn cmd_log() { AppHandler::handle(&AppHandler); }
                    "#,
            ),
        ],
        "cmd_log",
        "crate::application::handler::Greeting::handle",
        false,
    ),
    (
        "ambiguous multi-trait default leaves Logging unresolved",
        &[
            (
                "src/application/handler.rs",
                r#"
                    pub trait Greeting { fn handle(&self) {} }
                    pub trait Logging { fn handle(&self) {} }
                    "#,
            ),
            (
                "src/application/logging.rs",
                r#"
                    use crate::application::handler::{Greeting, Logging};
                    pub struct AppHandler;
                    impl Greeting for AppHandler {}
                    impl Logging for AppHandler {}
                    "#,
            ),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::logging::AppHandler;
                    pub fn cmd_log() { AppHandler::handle(&AppHandler); }
                    "#,
            ),
        ],
        "cmd_log",
        "crate::application::handler::Logging::handle",
        false,
    ),
    (
        // sanity: a real concrete target pub-fn must remain a boundary
        "real target-layer pub fn is a touchpoint",
        &[
            ("src/application/stats.rs", "pub fn get_stats() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::stats::get_stats;
                    pub fn cmd_stats() { get_stats(); }
                    "#,
            ),
        ],
        "cmd_stats",
        "crate::application::stats::get_stats",
        true,
    ),
];

#[test]
fn touchpoint_anchor_membership() {
    for (label, files, handler, anchor, present) in TOUCHPOINT_ANCHOR_CASES {
        let tps = tp_ports(files, handler);
        assert_eq!(
            tps.contains(*anchor),
            *present,
            "case {label}: anchor {anchor} membership; got {tps:?}"
        );
    }
}
