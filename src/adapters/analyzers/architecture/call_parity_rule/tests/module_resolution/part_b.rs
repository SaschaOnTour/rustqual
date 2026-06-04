use super::*;

const MODULE_EDGE_CASES_B: &[EdgeCase] = &[
    (
        // `application/orphan.rs` exists but `mod orphan;` is never
        // declared → not a real sibling submodule.
        "orphan file does not act as a sibling submodule",
        &[
            (
                "src/application/mod.rs",
                r#"
                    use orphan::Local;

                    pub fn dispatch() {
                        Local::run();
                    }
                    "#,
            ),
            (
                "src/application/orphan.rs",
                r#"
                    pub struct Local;
                    impl Local {
                        pub fn run() {}
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
        "crate::application::orphan::Local::run",
        false,
    ),
    (
        // `mod response;` declared inside `hidden.rs` is a real child
        // for code in that file, even though the ancestor chain is
        // hidden by a non-root private `mod hidden;`.
        "private-mod sibling import resolves with a hidden ancestor chain",
        &[
            ("src/lib.rs", "pub mod application;\n"),
            ("src/application/mod.rs", "mod hidden;\n"),
            (
                "src/application/hidden.rs",
                r#"
                    mod response;

                    use response::Local;

                    pub fn dispatch() {
                        Local::run();
                    }
                    "#,
            ),
            (
                "src/application/hidden/response.rs",
                r#"
                    pub struct Local;
                    impl Local {
                        pub fn run() {}
                    }
                    "#,
            ),
        ],
        "crate::application::hidden::dispatch",
        "crate::application::hidden::response::Local::run",
        true,
    ),
    (
        "pub use re-export forwards the call to the real definition",
        &[
            (
                "src/application/middleware/mod.rs",
                r#"
                    pub mod savings_recorder;
                    pub use savings_recorder::record_operation;
                    "#,
            ),
            (
                "src/application/middleware/savings_recorder.rs",
                "pub fn record_operation() {}",
            ),
            (
                "src/application/session.rs",
                r#"
                    use crate::application::middleware;
                    pub struct Session;
                    impl Session {
                        pub fn search(&self) {
                            middleware::record_operation();
                        }
                    }
                    "#,
            ),
        ],
        "crate::application::session::Session::search",
        "crate::application::middleware::savings_recorder::record_operation",
        true,
    ),
    (
        // fallback walker (no lib.rs/main.rs): stale `foo/mod.rs` must
        // not register submodules once `foo.rs` is the tie-break winner
        "fallback walker skips a stale mod.rs when file.rs wins",
        &[
            (
                "src/foo.rs",
                r#"
                    use beta::T;
                    pub fn dispatch() { T::run(); }
                    "#,
            ),
            ("src/foo/mod.rs", "pub mod beta;\n"),
            (
                "src/foo/beta.rs",
                r#"
                    pub struct T;
                    impl T { pub fn run() {} }
                    "#,
            ),
        ],
        "crate::foo::dispatch",
        "crate::foo::beta::T::run",
        false,
    ),
    (
        // `use ::ext::A` is extern-rooted (leading `::`) → must not
        // re-canonicalise to a same-named workspace module
        "extern-rooted use alias does not re-canonicalise to the workspace",
        &[
            ("src/lib.rs", "pub mod ext;\npub mod app;\n"),
            (
                "src/ext.rs",
                r#"
                    pub struct A;
                    impl A { pub fn m() {} }
                    "#,
            ),
            (
                "src/app.rs",
                r#"
                    use ::ext::A as Local;
                    pub fn dispatch() { Local::m(); }
                    "#,
            ),
        ],
        "crate::app::dispatch",
        "crate::ext::A::m",
        false,
    ),
    (
        // both `foo.rs` and `foo/mod.rs` back `["foo"]`; `foo.rs` must
        // deterministically win and its `mod alpha;` be walked
        "file.rs wins over dir/mod.rs for the same module path",
        &[
            ("src/lib.rs", "pub mod foo;\n"),
            (
                "src/foo.rs",
                r#"
                    pub mod alpha;
                    use alpha::Local;
                    pub fn dispatch() { Local::run(); }
                    "#,
            ),
            (
                "src/foo/alpha.rs",
                r#"
                    pub struct Local;
                    impl Local { pub fn run() {} }
                    "#,
            ),
            ("src/foo/mod.rs", "pub mod beta;\n"),
            ("src/foo/beta.rs", "pub fn unused() {}\n"),
        ],
        "crate::foo::dispatch",
        "crate::foo::alpha::Local::run",
        true,
    ),
];

#[test]
fn module_path_resolution_edges_part_b() {
    run_edge_cases(MODULE_EDGE_CASES_B);
}
