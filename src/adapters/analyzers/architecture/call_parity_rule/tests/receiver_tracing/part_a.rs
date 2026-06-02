use super::*;

const RECEIVER_EDGE_CASES_A: &[EdgeCase] = &[
    (
        // self.ensure_session()? then Session::open inside it — two hops
        "self-method chain through a helper to an associated fn",
        &[
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
        ],
        &[
            (
                "crate::mcp::server::Server::search",
                "crate::mcp::server::Server::ensure_session",
            ),
            (
                "crate::mcp::server::Server::ensure_session",
                "crate::application::session::Session::open",
            ),
        ],
    ),
    (
        // same chain, but `pub async fn search` — async lowering must
        // not break the edge
        "async self-method chain traces the same edges",
        &[
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
        ],
        &[
            (
                "crate::mcp::server::Server::search",
                "crate::mcp::server::Server::ensure_session",
            ),
            (
                "crate::mcp::server::Server::ensure_session",
                "crate::application::session::Session::open",
            ),
        ],
    ),
    (
        // `let session = open()?; session.X()` in both `if` branches
        "locally-bound let receiver traces multiple method calls",
        &[
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
        ],
        &[
            (
                "crate::cli::handlers::cmd_replace",
                "crate::application::session::Session::replace_preview",
            ),
            (
                "crate::cli::handlers::cmd_replace",
                "crate::application::session::Session::replace_apply",
            ),
        ],
    ),
    (
        // `fn handle_replace(session: &Session, ...)` — param-typed receiver
        "parameter-bound receiver traces multiple method calls",
        &[
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
        ],
        &[
            (
                "crate::mcp::handlers::handle_replace",
                "crate::application::session::Session::replace_preview",
            ),
            (
                "crate::mcp::handlers::handle_replace",
                "crate::application::session::Session::replace_apply",
            ),
        ],
    ),
];

#[test]
fn receiver_tracing_emits_expected_edges_part_a() {
    run_edge_cases(RECEIVER_EDGE_CASES_A);
}
