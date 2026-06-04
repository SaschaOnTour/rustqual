use super::*;

const RECEIVER_EDGE_CASES_B: &[EdgeCase] = &[
    (
        // path call inside a generic fn body
        "generic-fn body traces a path call to a generic target",
        &[
            (
                "src/application/savings.rs",
                "pub fn record_file_op<T>(_r: &T, _meta: &str) {}",
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
        ],
        &[(
            "crate::application::middleware::record_operation",
            "crate::application::savings::record_file_op",
        )],
    ),
    (
        // path calls inside `match` arm bodies (one edge per arm)
        "path calls inside generic match-arm bodies trace edges",
        &[
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
        ],
        &[
            (
                "crate::application::middleware::record_operation",
                "crate::application::savings::record_file_op",
            ),
            (
                "crate::application::middleware::record_operation",
                "crate::application::savings::record_symbol_op",
            ),
        ],
    ),
    (
        // method bodies inside an `#[tool_router]`-decorated impl must
        // still contribute edges (syn records unknown attrs verbatim)
        "unknown attribute on an impl block does not hide inner calls",
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
        ],
        &[(
            "crate::mcp::server::Server::search",
            "crate::application::session::Session::open",
        )],
    ),
];

#[test]
fn receiver_tracing_emits_expected_edges_part_b() {
    run_edge_cases(RECEIVER_EDGE_CASES_B);
}
