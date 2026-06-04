use super::*;

const MODULE_EDGE_CASES_A: &[EdgeCase] = &[
    (
        "concrete UFCS via sibling-submodule import",
        &[
            (
                "src/application/mod.rs",
                r#"
                    pub mod response;

                    use serde::Serialize;
                    use response::ConcreteOutput;

                    pub fn dispatcher<T: Serialize>(value: &T) -> ConcreteOutput {
                        let body = serde_json::to_string(value).unwrap_or_default();
                        ConcreteOutput::new(body)
                    }
                    "#,
            ),
            (
                "src/application/response.rs",
                r#"
                    pub struct ConcreteOutput { pub body: String }
                    impl ConcreteOutput {
                        pub fn new(body: String) -> Self { Self { body } }
                    }
                    "#,
            ),
            (
                "src/cli/mod.rs",
                r#"
                    use crate::application::dispatcher;

                    pub fn cmd_run() -> String {
                        let out = dispatcher(&42u32);
                        out.body
                    }
                    "#,
            ),
        ],
        "crate::application::dispatcher",
        "crate::application::response::ConcreteOutput::new",
        true,
    ),
    (
        // Rust 2018+: `use response::Local` resolves to the local
        // sibling, NOT crate-root `crate::response`.
        "sibling submodule wins over same-leaf crate root (local)",
        &[
            (
                "src/response/mod.rs",
                r#"
                    pub struct OuterRoot;
                    impl OuterRoot {
                        pub fn run() {}
                    }
                    "#,
            ),
            (
                "src/application/response.rs",
                r#"
                    pub struct Local;
                    impl Local {
                        pub fn run() {}
                    }
                    "#,
            ),
            (
                "src/application/mod.rs",
                r#"
                    pub mod response;

                    use response::Local;

                    pub fn dispatch() {
                        Local::run();
                    }
                    "#,
            ),
            (
                "src/cli/mod.rs",
                r#"
                    use crate::application::dispatch;

                    pub fn cmd_run() {
                        dispatch();
                    }
                    "#,
            ),
        ],
        "crate::application::dispatch",
        "crate::application::response::Local::run",
        true,
    ),
    (
        "sibling submodule wins over same-leaf crate root (crate-root NOT routed)",
        &[
            (
                "src/response/mod.rs",
                r#"
                    pub struct OuterRoot;
                    impl OuterRoot {
                        pub fn run() {}
                    }
                    "#,
            ),
            (
                "src/application/response.rs",
                r#"
                    pub struct Local;
                    impl Local {
                        pub fn run() {}
                    }
                    "#,
            ),
            (
                "src/application/mod.rs",
                r#"
                    pub mod response;

                    use response::Local;

                    pub fn dispatch() {
                        Local::run();
                    }
                    "#,
            ),
            (
                "src/cli/mod.rs",
                r#"
                    use crate::application::dispatch;

                    pub fn cmd_run() {
                        dispatch();
                    }
                    "#,
            ),
        ],
        "crate::application::dispatch",
        "crate::response::OuterRoot::run",
        false,
    ),
    (
        "inline-mod sibling import traces the edge",
        &[
            (
                "src/application/mod.rs",
                r#"
                    pub mod outer {
                        pub mod inner {
                            pub struct Local;
                            impl Local {
                                pub fn run() {}
                            }
                        }

                        use inner::Local;

                        pub fn dispatch() {
                            Local::run();
                        }
                    }
                    "#,
            ),
            (
                "src/cli/mod.rs",
                r#"
                    use crate::application::outer::dispatch;

                    pub fn cmd_run() {
                        dispatch();
                    }
                    "#,
            ),
        ],
        "crate::application::outer::dispatch",
        "crate::application::outer::inner::Local::run",
        true,
    ),
];

#[test]
fn module_path_resolution_edges_part_a() {
    run_edge_cases(MODULE_EDGE_CASES_A);
}
