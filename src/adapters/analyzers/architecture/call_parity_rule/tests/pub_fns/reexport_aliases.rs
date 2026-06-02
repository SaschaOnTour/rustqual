use super::*;

#[test]
fn test_collect_pub_fns_records_impl_via_renamed_stdlib_wrapper() {
    // `use std::sync::Arc as Shared; pub type Public = Shared<private::Hidden>;`
    // — the visibility pass must follow the import alias when peeling
    // wrappers, otherwise `Shared` is treated as a non-wrapper and
    // `private::Hidden` never enters `visible_canonicals`.
    let file = parse(
        r#"
        use std::sync::Arc as Shared;
        mod private {
            pub struct Hidden;
            impl Hidden {
                pub fn op(&self) {}
            }
        }
        pub type Public = Shared<private::Hidden>;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "renamed stdlib wrapper alias must peel in visibility pass, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_records_impl_via_pub_type_alias_through_wrapper() {
    // `pub type Public = Box<private::Hidden>;` — the alias target
    // is wrapped in a Deref-transparent smart pointer. Receiver
    // resolution peels Box/Arc/Rc/Cow, so the visible-types pass
    // must do the same to reach the inner `private::Hidden` and
    // recognise its impl methods as adapter surface.
    let file = parse(
        r#"
        mod private {
            pub struct Hidden;
            impl Hidden {
                pub fn op(&self) {}
            }
        }
        pub type Public = Box<private::Hidden>;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "wrapper-alias target's impl method must be recorded, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_records_impl_via_pub_type_alias() {
    // `pub type Public = private::Hidden;` exposes a hidden source
    // type's methods through the alias. Receiver-type inference
    // already resolves `Public` to its target, so the only piece
    // missing for Check B was visibility — register the target's
    // canonical alongside the alias path.
    let file = parse(
        r#"
        mod private {
            pub struct Hidden;
            impl Hidden {
                pub fn op(&self) {}
            }
        }
        pub type Public = private::Hidden;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "impl on pub-type-alias target must be recorded, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_records_renamed_reexport_impl_methods() {
    // `pub use private::Hidden as PublicHidden;` re-exports the
    // source type under a new name. The impl uses the original
    // `Hidden`, so visibility must resolve through the re-export
    // path — short-name matching against `PublicHidden` would miss
    // the impl. Recording must work via the source-canonical path
    // that both sides agree on.
    let file = parse(
        r#"
        mod private {
            pub struct Hidden;
        }
        impl private::Hidden {
            pub fn op(&self) {}
        }
        pub use private::Hidden as PublicHidden;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "renamed re-export must still expose impl method, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_chases_reexported_type_alias_to_target() {
    // `pub use private::Public;` where `Public` is a type alias for
    // `private::Hidden` and `op` is defined on `Hidden`. Receiver-type
    // inference resolves callers `x: Public` to `Hidden::op`, so the
    // visibility set must contain BOTH `Public` (the alias) and
    // `Hidden` (its target) — otherwise Check B would drop `Hidden::op`
    // even though it is reachable through the public alias.
    let file = parse(
        r#"
        mod private {
            pub struct Hidden;
            pub type Public = Hidden;
            impl Hidden {
                pub fn op(&self) {}
            }
        }
        pub use private::Public;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "re-exported type alias must surface its target's impl methods, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_records_pub_use_reexport_with_qualified_impl() {
    // `pub use private::Hidden;` with the impl at file level
    // (qualified `impl private::Hidden { … }`) — the re-export
    // resolves to the source-canonical `crate::file::private::Hidden`
    // and registers in `visible_canonicals`. The impl resolves to
    // the same canonical, so the methods record. With canonical-path
    // matching, impls *inside* `mod private` for the same re-exported
    // type also record correctly (the mod's own visibility no longer
    // gates impl methods).
    let file = parse(
        r#"
        mod private {
            pub struct Hidden;
        }
        impl private::Hidden {
            pub fn op(&self) {}
        }
        pub use private::Hidden;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "re-exported type with file-level impl must be recorded, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_skips_trait_impl_method_on_private_self_type() {
    // `impl PubTrait for Hidden { fn handle() }` — `Hidden` is private,
    // so `Hidden::handle` is NOT registered as a target pub-fn.
    // Dispatch through `dyn PubTrait` no longer emits `Hidden::handle`
    // either; instead it emits the synthetic anchor
    // `<PubTrait>::handle` which represents the capability.
    // Registering private-self-type impl-methods as target pub-fns
    // would force adapter coverage checks for implementation details
    // that are unreachable through the public API.
    let file = parse(
        r#"
        pub trait PubTrait {
            fn handle(&self);
        }
        struct Hidden;
        impl PubTrait for Hidden {
            fn handle(&self) {}
        }
        "#,
    );
    let files = vec![("src/application/mod.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let app = names_for_layer(&by_layer, "application");
    assert!(
        !app.contains("handle"),
        "trait-impl method on private self type must stay out of pub-fn set; dispatch reaches it via the trait-method anchor, not as a concrete target pub-fn. Got {app:?}"
    );
}

#[test]
fn test_collect_pub_fns_records_inherited_trait_impl_methods() {
    // `impl PubTrait for X { fn handle(&self) {} }` — the impl-item
    // `vis` is `Inherited`, but the method is part of the public
    // surface because the trait is public. Otherwise dispatch could
    // emit `X::handle` as a touchpoint while `X::handle` never enters
    // the target pub-fn set, hiding peer-adapter coverage gaps in
    // Check B/D.
    let file = parse(
        r#"
        pub trait PubTrait {
            fn handle(&self);
        }
        pub struct X;
        impl PubTrait for X {
            fn handle(&self) {}
        }
        "#,
    );
    let files = vec![("src/application/mod.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let app = names_for_layer(&by_layer, "application");
    assert!(
        app.contains("handle"),
        "trait-impl method must be recorded as pub fn even with Inherited vis, got {app:?}"
    );
}

#[test]
fn test_collect_pub_fns_unions_lib_and_main_root_trees() {
    // Workspace with both `src/lib.rs` and `src/main.rs`. `lib.rs`
    // declares `mod application;` privately (visible at crate-root
    // level per the relaxation), `main.rs` declares `mod cli;`
    // privately. A file is visible in its respective tree; the two
    // trees stay independent.
    let lib = parse("mod application;");
    let main = parse("mod cli;");
    let app = parse("pub fn search() {}");
    let cli = parse("pub fn cmd() {}");
    let files = vec![
        ("src/lib.rs", &lib),
        ("src/main.rs", &main),
        ("src/application/mod.rs", &app),
        ("src/cli/mod.rs", &cli),
    ];
    let by_layer = pub_fns_by_layer(&files);
    let app_fns = names_for_layer(&by_layer, "application");
    let cli_fns = names_for_layer(&by_layer, "cli");
    assert!(
        app_fns.contains("search"),
        "application module declared in lib.rs root must surface its pub fns, got {app_fns:?}"
    );
    assert!(
        cli_fns.contains("cmd"),
        "cli module declared in main.rs root must surface its pub fns, got {cli_fns:?}"
    );
}

#[test]
fn test_collect_pub_fns_includes_crate_root_mod_decl_without_pub() {
    // `src/lib.rs` typically writes `mod cli; mod application;` —
    // sibling modules still reach them via `crate::cli::…`, and
    // call-parity is an internal architecture check. The visibility
    // pass must therefore treat crate-root `mod X;` (without `pub`)
    // as visible so adapter handlers in `src/cli/handlers.rs` are
    // recorded as pub-fns and Checks A/B/C/D run against them.
    let lib = parse("mod cli; mod application;");
    let cli_mod = parse("pub fn cmd_search() {}");
    let app_mod = parse("pub fn search() {}");
    let files = vec![
        ("src/lib.rs", &lib),
        ("src/cli/mod.rs", &cli_mod),
        ("src/application/mod.rs", &app_mod),
    ];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    let app = names_for_layer(&by_layer, "application");
    assert!(
        cli.contains("cmd_search"),
        "crate-root `mod cli;` (no pub) must still expose adapter pub-fns, got {cli:?}"
    );
    assert!(
        app.contains("search"),
        "crate-root `mod application;` (no pub) must still expose target pub-fns, got {app:?}"
    );
}
