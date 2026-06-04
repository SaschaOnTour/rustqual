use super::*;

#[test]
fn test_collect_pub_fns_skips_impl_method_on_type_in_private_inline_mod() {
    // `mod private { pub struct Hidden; impl Hidden { pub fn op() {} } }`
    // — `Hidden` is pub but only inside a private mod, so its
    // workspace-visible-types entry must NOT register, and the impl
    // method `op` must not appear as adapter surface.
    let file = parse(
        r#"
        mod private {
            pub struct Hidden;
            impl Hidden {
                pub fn op(&self) {}
            }
        }
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        !cli.contains("op"),
        "impl method on type in private mod must be skipped, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_records_impl_in_private_mod_for_public_type() {
    // `pub struct Session` at file level, but its `impl` block lives
    // inside a private inline mod via `super::Session`. Rust treats
    // `s.diff()` as callable from any caller that can name `Session`,
    // so the public type's pub inherent methods must be recorded as
    // adapter surface — even though the impl block itself sits in a
    // private mod.
    let file = parse(
        r#"
        pub struct Session;
        mod methods {
            impl super::Session {
                pub fn diff(&self) {}
            }
        }
        "#,
    );
    let files = vec![("src/application/session.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let app = names_for_layer(&by_layer, "application");
    assert!(
        app.contains("diff"),
        "impl in private mod for public type must be recorded, got {app:?}"
    );
}

#[test]
fn test_collect_pub_fns_records_impl_via_nested_pub_use_export_path() {
    // `pub mod outer { pub use self::private::Hidden; }` re-exports
    // `Hidden` at `crate::file::outer::Hidden`. An impl written
    // against the export path must be recognised — visible_canonicals
    // needs both the source path *and* the export path so impl
    // resolution doesn't miss it.
    let file = parse(
        r#"
        pub mod outer {
            mod private {
                pub struct Hidden;
            }
            pub use self::private::Hidden;
        }
        impl outer::Hidden {
            pub fn op(&self) {}
        }
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "impl on nested-mod re-export path must be recorded, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_records_impl_via_chained_type_alias() {
    // `type Inner = private::Hidden; pub type Public = Inner;` —
    // the alias chain must be followed to the source type, otherwise
    // visible_canonicals only contains `Inner` and the impl on
    // `Hidden` stays out of scope.
    let file = parse(
        r#"
        mod private {
            pub struct Hidden;
            impl Hidden {
                pub fn op(&self) {}
            }
        }
        type Inner = private::Hidden;
        pub type Public = Inner;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "alias chain target's impl method must be recorded, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_does_not_promote_bare_local_arc() {
    // `use crate::wrap::Arc; pub type Public = Arc<private::Hidden>;`
    // — bare `Arc` is shadowed by the local `use`. Visibility must
    // canonicalise first and refuse to auto-peel local Arcs.
    let file = parse(
        r#"
        mod wrap { pub struct Arc<T>(T); }
        use crate::wrap::Arc;
        mod private {
            pub struct Hidden;
            impl Hidden {
                pub fn op(&self) {}
            }
        }
        pub type Public = Arc<private::Hidden>;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        !cli.contains("op"),
        "bare Arc shadowed by local must not auto-peel, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_does_not_promote_qualified_local_arc() {
    // `pub type Public = wrap::Arc<private::Hidden>;` — `wrap::Arc`
    // is a *local* wrapper, not stdlib. Direct dispatch on the leaf
    // `Arc` must NOT peel; otherwise Check B would require coverage
    // for methods on `private::Hidden` that aren't actually exposed.
    let file = parse(
        r#"
        mod wrap { pub struct Arc<T>(T); }
        mod private {
            pub struct Hidden;
            impl Hidden {
                pub fn op(&self) {}
            }
        }
        pub type Public = wrap::Arc<private::Hidden>;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        !cli.contains("op"),
        "qualified local Arc must not auto-peel as stdlib Arc, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_does_not_promote_local_wrapper_alias() {
    // `use crate::wrap::Arc as Shared;` aliases a *local* wrapper
    // type — its canonical (`crate::wrap::Arc`) doesn't start with
    // std/core/alloc, so the visibility pass must NOT auto-peel it
    // when it appears in `pub type Public = Shared<…>`. Only stdlib
    // wrappers are auto-peeled; user-configured wrappers stay
    // last-segment based.
    let file = parse(
        r#"
        use crate::wrap::Arc as Shared;
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
        !cli.contains("op"),
        "local wrapper alias must not auto-peel as stdlib Arc, got {cli:?}"
    );
}
