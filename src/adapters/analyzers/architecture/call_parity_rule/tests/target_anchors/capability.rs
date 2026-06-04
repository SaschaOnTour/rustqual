use super::*;

// Which `<Trait>::<method>` anchors count as target-layer (application)
// capabilities for Check B/D. An anchor qualifies when the trait is
// workspace-visible AND its callable body (default body, or an overriding impl)
// lives in the target layer; it's rejected when the body is outside target, the
// trait is private/peer-adapter-declared, or the method is pure signature with
// no body anywhere. (label, files, adapters, anchor, present)
const ANCHOR_CAP_CASES: &[AnchorCapCase] = &[
    (
        // trait in ports, overriding impl in application (target)
        "ports trait + overriding target impl",
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
        ],
        &[],
        "crate::ports::handler::Handler::handle",
        true,
    ),
    (
        // impl in cli (adapter layer), not target → no target capability
        "ports trait + impl in adapter layer only",
        &[
            (
                "src/ports/cli_only.rs",
                "pub trait CliOnly { fn handle(&self); }",
            ),
            (
                "src/cli/impls.rs",
                r#"
                    use crate::ports::cli_only::CliOnly;
                    pub struct CliImpl;
                    impl CliOnly for CliImpl { fn handle(&self) {} }
                    "#,
            ),
        ],
        &[],
        "crate::ports::cli_only::CliOnly::handle",
        false,
    ),
    (
        // trait declared inside a peer-adapter (mcp) layer → filtered out
        "peer-adapter-declared trait (target impl) rejected",
        &[
            (
                "src/mcp/handler.rs",
                "pub trait Handler { fn handle(&self); }",
            ),
            (
                "src/application/logging.rs",
                r#"
                    use crate::mcp::handler::Handler;
                    pub struct LoggingHandler;
                    impl Handler for LoggingHandler { fn handle(&self) {} }
                    "#,
            ),
        ],
        &["cli", "mcp"],
        "crate::mcp::handler::Handler::handle",
        false,
    ),
    (
        // default body in the target-layer trait itself, no impls
        "default-only target-layer trait",
        &[(
            "src/application/handler.rs",
            "pub trait Handler { fn handle(&self) {} }",
        )],
        &[],
        "crate::application::handler::Handler::handle",
        true,
    ),
    (
        // private (non-pub) trait isn't workspace-visible
        "private target-layer trait",
        &[(
            "src/application/internal.rs",
            "trait Internal { fn run(&self) {} }",
        )],
        &[],
        "crate::application::internal::Internal::run",
        false,
    ),
    (
        // private ports trait, even with a target impl, stays invisible
        "private ports trait + target impl",
        &[
            ("src/ports/internal.rs", "trait Hidden { fn run(&self); }"),
            (
                "src/application/impls.rs",
                r#"
                    use crate::ports::internal::Hidden;
                    pub struct Impl;
                    impl Hidden for Impl { fn run(&self) {} }
                    "#,
            ),
        ],
        &[],
        "crate::ports::internal::Hidden::run",
        false,
    ),
    (
        // empty target impl inherits a ports default body that never
        // crosses into target → not a target capability
        "inherited default body lives outside target",
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
        ],
        &[],
        "crate::ports::handler::Handler::handle",
        false,
    ),
    (
        // pub(crate) traits are workspace-visible per `is_visible`
        "pub(crate) ports trait + target impl",
        &[
            (
                "src/ports/handler.rs",
                "pub(crate) trait Handler { fn handle(&self); }",
            ),
            (
                "src/application/logging.rs",
                r#"
                    use crate::ports::handler::Handler;
                    pub struct LoggingHandler;
                    impl Handler for LoggingHandler { fn handle(&self) {} }
                    "#,
            ),
        ],
        &[],
        "crate::ports::handler::Handler::handle",
        true,
    ),
    (
        // `pub trait` inside a private `mod` is workspace-invisible
        "pub trait inside a private mod",
        &[(
            "src/application/wrapper.rs",
            r#"
                mod inner {
                    pub trait T { fn run(&self) {} }
                }
                "#,
        )],
        &[],
        "crate::application::wrapper::inner::T::run",
        false,
    ),
    (
        // pure signature, no default body and no impl → uncallable
        "signature-only target-layer trait",
        &[(
            "src/application/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        )],
        &[],
        "crate::application::handler::Handler::handle",
        false,
    ),
];

#[test]
fn target_anchor_capability_enumeration() {
    for (label, files, adapters, anchor, present) in ANCHOR_CAP_CASES {
        let caps = anchor_caps(files, adapters);
        assert_eq!(
            caps.contains(*anchor),
            *present,
            "case {label}: anchor {anchor} presence; got {caps:?}"
        );
    }
}

#[test]
fn populate_anchor_index_resolves_impl_layers_for_cross_file_impls() {
    // Trait declared in `ports/handler.rs`, two impls in different
    // application files. The anchor's resolved layer set MUST include
    // `application` even though the trait and impls live in distinct
    // files — `populate_anchor_index` resolves each impl canonical
    // through `LayerDefinitions::layer_of_crate_path`, not via the
    // graph's per-node layer cache (which is built from edges, not
    // bare struct types).
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/a.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct A;
            impl Handler for A { fn handle(&self) {} }
            "#,
        ),
        (
            "src/application/b.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct B;
            impl Handler for B { fn handle(&self) {} }
            "#,
        ),
    ]);
    let graph = graph_of(&ws);
    let anchor = "crate::ports::handler::Handler::handle";
    let info = graph.trait_method_anchors.get(anchor).unwrap_or_else(|| {
        panic!(
            "anchor not registered, got {:?}",
            graph.trait_method_anchors
        )
    });
    assert!(
        info.impl_layers.contains("application"),
        "cross-file impls in application must resolve to layer `application`, got {:?}",
        info.impl_layers
    );
}
