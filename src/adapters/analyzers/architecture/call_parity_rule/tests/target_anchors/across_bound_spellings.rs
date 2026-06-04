use super::*;

const ACROSS_BOUND_SPELLINGS_CASES: &[EdgeCase] = &[
    (
        // `where Q: T` instead of inline `<Q: T>` — spelling must not matter
        "where-clause bound on generic-param receiver",
        &[(
            "src/application/mod.rs",
            r#"
                pub trait MethodTrait {
                    fn execute(&self, input: &str) -> String;
                }

                pub fn dispatch_method<Q>(query: &Q, input: &str) -> String
                where
                    Q: MethodTrait,
                {
                    query.execute(input)
                }
                "#,
        )],
        "crate::application::dispatch_method",
        "crate::application::MethodTrait::execute",
    ),
    (
        // bound lives on the impl block, method-call inside the body
        "impl-level generic bound on method-call receiver",
        &[(
            "src/application/mod.rs",
            r#"
                pub trait MethodTrait {
                    fn execute(&self, input: &str) -> String;
                }

                pub struct Runner<Q>(pub Q);

                impl<Q: MethodTrait> Runner<Q> {
                    pub fn run(&self, q: &Q, input: &str) -> String {
                        q.execute(input)
                    }
                }
                "#,
        )],
        "crate::application::Runner::run",
        "crate::application::MethodTrait::execute",
    ),
    (
        // `Q: Audit + Handler` — `q.handle()` must hit the SECOND bound
        // `Handler`, even though `Audit` (first) lacks the method
        "multi-bound generic receiver hits the defining bound",
        &[
            (
                "src/application/mod.rs",
                r#"
                    pub trait Audit {
                        fn audit(&self);
                    }

                    pub trait Handler {
                        fn handle(&self);
                    }

                    pub fn dispatch<Q: Audit + Handler>(q: &Q) {
                        q.handle();
                    }
                    "#,
            ),
            (
                "src/cli/mod.rs",
                r#"
                    use crate::application::{dispatch, Audit, Handler};

                    pub struct Real;
                    impl Audit for Real {
                        fn audit(&self) {}
                    }
                    impl Handler for Real {
                        fn handle(&self) {}
                    }

                    pub fn cmd_run() {
                        dispatch(&Real);
                    }
                    "#,
            ),
        ],
        "crate::application::dispatch",
        "crate::application::Handler::handle",
    ),
];

#[test]
fn method_call_on_generic_receiver_emits_anchor_across_bound_spellings() {
    for (label, files, caller, anchor) in ACROSS_BOUND_SPELLINGS_CASES {
        let graph = build_graph_only(
            &build_workspace(files),
            &three_layer(),
            &empty_cfg_test(),
            &HashSet::new(),
        );
        assert!(
            graph_contains_edge(&graph, caller, anchor),
            "case {label}: generic-param dispatch must emit trait-anchor edge \
             {caller} → {anchor}; callees: {:?}",
            callees_of(&graph, caller),
        );
    }
}
