use super::*;

// Under the hexagonal layout, multiplicity drift (cli=2, mcp=1) must surface
// whether all adapters dispatch via `dyn Handler.handle()` (the drift lands on
// the trait anchor) or all call the concrete impl method directly via UFCS (the
// drift lands on the concrete canonical). Without anchor iteration / the
// conditional concrete-skip, these would be silent. (label, files, target)
const ANCHOR_MULT_CASES: &[AnchorMultCase] = &[
    (
        "trait dispatch → drift on the anchor",
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
                    pub fn cmd_dispatch_a(h: &dyn Handler) { h.handle(); }
                    pub fn cmd_dispatch_b(h: &dyn Handler) { h.handle(); }
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
        "crate::ports::handler::Handler::handle",
    ),
    (
        "all-direct concrete → drift on the concrete canonical",
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
                    pub fn cmd_log_a() { LoggingHandler::handle(&LoggingHandler); }
                    pub fn cmd_log_b() { LoggingHandler::handle(&LoggingHandler); }
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
        "crate::application::logging::LoggingHandler::handle",
    ),
];

#[test]
fn multiplicity_mismatch_on_trait_dispatch_and_direct_concrete() {
    for (label, files, target) in ANCHOR_MULT_CASES {
        let entries = multiplicity_ports(files);
        let entry = entries
            .iter()
            .find(|(t, _)| t == target)
            .unwrap_or_else(|| {
                panic!("case {label}: missed multiplicity drift on {target}; got {entries:?}")
            });
        assert_eq!(
            count_for(&entry.1, "cli"),
            Some(2),
            "case {label}: cli count"
        );
        assert_eq!(
            count_for(&entry.1, "mcp"),
            Some(1),
            "case {label}: mcp count"
        );
    }
}
