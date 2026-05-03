//! Tests for `collect_pub_fns_by_layer` — workspace-wide pub-fn
//! enumeration grouped by architecture layer.

use super::support::three_layer;
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::{
    collect_pub_fns_by_layer, PubFnInfo,
};
use crate::adapters::shared::use_tree::gather_alias_map;
use std::collections::{HashMap, HashSet};

/// Build an `aliases_per_file` map from a workspace slice — mirrors
/// what the call-parity entry point computes.
fn aliases_from_files(
    files: &[(&str, &syn::File)],
) -> HashMap<String, HashMap<String, Vec<String>>> {
    files
        .iter()
        .map(|(p, f)| (p.to_string(), gather_alias_map(f)))
        .collect()
}

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse file")
}

fn names_for_layer<'ast>(
    by_layer: &std::collections::HashMap<String, Vec<PubFnInfo<'ast>>>,
    layer: &str,
) -> HashSet<String> {
    by_layer
        .get(layer)
        .map(|fns| fns.iter().map(|f| f.fn_name.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn test_collect_pub_fns_in_layer_free_fn() {
    let file = parse(
        r#"
        pub fn cmd_stats() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(cli.contains("cmd_stats"), "cli = {cli:?}");
}

#[test]
fn test_collect_pub_fns_skips_private_fns() {
    let file = parse(
        r#"
        fn helper() {}
        pub fn cmd_stats() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(cli.contains("cmd_stats"));
    assert!(!cli.contains("helper"), "private fn must be skipped");
}

#[test]
fn test_pub_crate_is_treated_as_public_for_intra_crate_layers() {
    // Workspace-internal crates rely on `pub(crate)` for their architecture
    // surface; the check must treat any visibility-modified fn as
    // "visible enough" — only the implicit (no-modifier) case is private.
    let file = parse(
        r#"
        pub(crate) fn cmd_stats() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(cli.contains("cmd_stats"), "pub(crate) must be collected");
}

#[test]
fn test_pub_super_and_pub_in_path_treated_as_public() {
    let file = parse(
        r#"
        pub(super) fn cmd_a() {}
        pub(in crate::cli) fn cmd_b() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(cli.contains("cmd_a"));
    assert!(cli.contains("cmd_b"));
}

#[test]
fn test_collect_pub_fns_collects_pub_impl_methods_for_pub_type() {
    let file = parse(
        r#"
        pub struct Session;
        impl Session {
            pub fn search(&self) {}
            fn helper(&self) {}
        }
        "#,
    );
    let files = vec![("src/application/session.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let app = names_for_layer(&by_layer, "application");
    assert!(app.contains("search"), "pub impl method must be collected");
    assert!(
        !app.contains("helper"),
        "private impl method must be skipped"
    );
}

#[test]
fn test_collect_pub_fns_recognises_impl_across_files() {
    // Regression: `pub struct Session` in one file, `impl Session { pub
    // fn search() }` in another — both sides resolve to the same
    // canonical via the impl-file's `use` statement, so Check B sees
    // the impl methods as adapter surface. (Without the `use`, the
    // impl wouldn't compile in real Rust either.)
    let decl_file = parse("pub struct Session;");
    let impl_file = parse(
        r#"
        use crate::application::session::Session;
        impl Session {
            pub fn search(&self) {}
        }
        "#,
    );
    let files = vec![
        ("src/application/session.rs", &decl_file),
        ("src/application/session_impls.rs", &impl_file),
    ];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let app = names_for_layer(&by_layer, "application");
    assert!(
        app.contains("search"),
        "cross-file impl on pub type must be collected, got {app:?}"
    );
}

#[test]
fn test_collect_pub_fns_skips_impl_methods_on_private_type() {
    // Conservative: if the enclosing `impl Type { ... }` is for a private
    // (no-modifier) type, its pub methods aren't really reachable from
    // outside the file, so they're excluded from the call-parity scope.
    let file = parse(
        r#"
        struct Session;
        impl Session {
            pub fn search(&self) {}
        }
        "#,
    );
    let files = vec![("src/application/session.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let app = names_for_layer(&by_layer, "application");
    assert!(
        !app.contains("search"),
        "impl on private type must be skipped"
    );
}

#[test]
fn test_collect_pub_fns_groups_by_layer() {
    let cli_file = parse("pub fn cmd_stats() {}");
    let mcp_file = parse("pub fn handle_stats() {}");
    let app_file = parse("pub fn get_stats() {}");
    let files = vec![
        ("src/cli/handlers.rs", &cli_file),
        ("src/mcp/handlers.rs", &mcp_file),
        ("src/application/stats.rs", &app_file),
    ];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    assert_eq!(
        names_for_layer(&by_layer, "cli"),
        ["cmd_stats".to_string()].into()
    );
    assert_eq!(
        names_for_layer(&by_layer, "mcp"),
        ["handle_stats".to_string()].into()
    );
    assert_eq!(
        names_for_layer(&by_layer, "application"),
        ["get_stats".to_string()].into()
    );
}

#[test]
fn test_collect_pub_fns_skips_cfg_test_files() {
    let file = parse("pub fn cmd_stats() {}");
    let files = vec![("src/cli/handlers.rs", &file)];
    let mut cfg_test = HashSet::new();
    cfg_test.insert("src/cli/handlers.rs".to_string());
    let aliases = aliases_from_files(&files);
    let by_layer =
        collect_pub_fns_by_layer(&files, &aliases, &three_layer(), &cfg_test, &HashSet::new());
    assert!(
        names_for_layer(&by_layer, "cli").is_empty(),
        "cfg-test file must be skipped wholesale"
    );
}

#[test]
fn test_collect_pub_fns_skips_test_attr_fns() {
    let file = parse(
        r#"
        #[test]
        pub fn not_a_handler() {}
        pub fn cmd_stats() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(cli.contains("cmd_stats"));
    assert!(!cli.contains("not_a_handler"), "#[test] fn must be skipped");
}

#[test]
fn test_collect_pub_fns_skips_unmatched_files() {
    // File not covered by any layer — its pub fns are not part of the
    // call-parity scope (neither as adapter member nor as target).
    let file = parse("pub fn free_floating() {}");
    let files = vec![("src/utils/misc.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    for layer in ["application", "cli", "mcp"] {
        assert!(
            names_for_layer(&by_layer, layer).is_empty(),
            "layer {layer} must be empty"
        );
    }
}

#[test]
fn test_collect_pub_fns_skips_pub_fn_inside_private_inline_mod() {
    // `mod private { pub fn helper() {} }` — `helper` is `pub` but
    // its parent `mod private` has inherited visibility, so the fn
    // isn't reachable from outside the parent module. Must not be
    // recorded as adapter / target surface.
    let file = parse(
        r#"
        mod private {
            pub fn helper() {}
        }
        pub fn visible_top() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("visible_top"),
        "top-level pub fn must be recorded, got {cli:?}"
    );
    assert!(
        !cli.contains("helper"),
        "pub fn inside private inline mod must be skipped, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_treats_pub_self_as_private() {
    // `pub(self) fn helper()` is semantically private — equivalent to
    // inherited visibility. Must not be recorded.
    let file = parse(
        r#"
        pub(self) fn helper() {}
        pub fn visible() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("visible"),
        "plain `pub fn` must be recorded, got {cli:?}"
    );
    assert!(
        !cli.contains("helper"),
        "`pub(self) fn` is private-equivalent and must be skipped, got {cli:?}"
    );
}

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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        !cli.contains("op"),
        "bare Arc shadowed by local must not auto-peel, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_peels_qualified_user_wrapper() {
    // `pub type Public = axum::extract::State<private::Hidden>;` with
    // `transparent_wrappers = ["State"]`. External `axum::*` paths
    // can't be canonicalised, so the visibility pass must fall back
    // to last-segment matching for user-transparent wrappers.
    let file = parse(
        r#"
        mod private {
            pub struct Hidden;
            impl Hidden {
                pub fn op(&self) {}
            }
        }
        pub type Public = axum::extract::State<private::Hidden>;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let mut wrappers = HashSet::new();
    wrappers.insert("State".to_string());
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(&files, &aliases, &three_layer(), &HashSet::new(), &wrappers)
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "fully-qualified user wrapper must peel via leaf, got {cli:?}"
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        !cli.contains("op"),
        "local wrapper alias must not auto-peel as stdlib Arc, got {cli:?}"
    );
}

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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "renamed stdlib wrapper alias must peel in visibility pass, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_records_impl_via_pub_type_alias_through_user_wrapper() {
    // `pub type Public = State<private::Hidden>;` with a
    // user-configured transparent wrapper `State`. The visibility
    // pass must consult the same wrapper set the receiver resolver
    // uses, otherwise Check B drops the public target.
    let file = parse(
        r#"
        mod private {
            pub struct Hidden;
            impl Hidden {
                pub fn op(&self) {}
            }
        }
        pub type Public = State<private::Hidden>;
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let mut wrappers = HashSet::new();
    wrappers.insert("State".to_string());
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(&files, &aliases, &three_layer(), &HashSet::new(), &wrappers)
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "user-wrapper alias target's impl method must be recorded, got {cli:?}"
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
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
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "re-exported type with file-level impl must be recorded, got {cli:?}"
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
    let aliases = aliases_from_files(&files);
    let by_layer = collect_pub_fns_by_layer(
        &files,
        &aliases,
        &three_layer(),
        &HashSet::new(),
        &HashSet::new(),
    );
    let app = names_for_layer(&by_layer, "application");
    assert!(
        app.contains("handle"),
        "trait-impl method must be recorded as pub fn even with Inherited vis, got {app:?}"
    );
}

#[test]
fn test_collect_pub_fns_excludes_orphan_file_not_declared_in_crate_root() {
    // `src/lib.rs` doesn't declare `mod application;`, but
    // `src/application/mod.rs` exists with a `pub fn helper()`.
    // The file is not actually part of any module tree — its pub
    // fns must NOT be recorded as adapter/target surface.
    let lib = parse("mod cli;");
    let cli = parse("pub fn cmd() {}");
    let app = parse("pub fn helper() {}");
    let files = vec![
        ("src/lib.rs", &lib),
        ("src/cli/mod.rs", &cli),
        ("src/application/mod.rs", &app),
    ];
    let aliases = aliases_from_files(&files);
    let by_layer = collect_pub_fns_by_layer(
        &files,
        &aliases,
        &three_layer(),
        &HashSet::new(),
        &HashSet::new(),
    );
    let app_fns = names_for_layer(&by_layer, "application");
    assert!(
        !app_fns.contains("helper"),
        "orphan file (no `mod application;` decl in lib.rs) must not contribute pub fns, got {app_fns:?}"
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
    let aliases = aliases_from_files(&files);
    let by_layer = collect_pub_fns_by_layer(
        &files,
        &aliases,
        &three_layer(),
        &HashSet::new(),
        &HashSet::new(),
    );
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
    let aliases = aliases_from_files(&files);
    let by_layer = collect_pub_fns_by_layer(
        &files,
        &aliases,
        &three_layer(),
        &HashSet::new(),
        &HashSet::new(),
    );
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

#[test]
fn test_collect_pub_fns_excludes_pub_fn_under_private_ancestor_chain() {
    // `mod internal;` at depth 1 (private) + `pub mod deep;` at depth 2.
    // Even though deep's direct parent says `pub`, the `internal`
    // ancestor is private — the whole subtree must be excluded.
    let app = parse("mod internal;");
    let internal = parse("pub mod deep;");
    let deep = parse("pub fn helper() {}");
    let files = vec![
        ("src/application/mod.rs", &app),
        ("src/application/internal/mod.rs", &internal),
        ("src/application/internal/deep.rs", &deep),
    ];
    let aliases = aliases_from_files(&files);
    let by_layer = collect_pub_fns_by_layer(
        &files,
        &aliases,
        &three_layer(),
        &HashSet::new(),
        &HashSet::new(),
    );
    let app_fns = names_for_layer(&by_layer, "application");
    assert!(
        !app_fns.contains("helper"),
        "private ancestor `mod internal;` must hide pub fns in deep descendants, got {app_fns:?}"
    );
}

#[test]
fn test_collect_pub_fns_excludes_pub_fn_in_file_backed_private_module() {
    // `mod internal;` (without `pub`) keeps `src/application/internal.rs`
    // private to the parent. `pub fn helper()` inside that file must
    // NOT be recorded as a target-layer pub fn — otherwise Check B/D
    // would require adapter coverage for a private helper.
    let parent = parse("mod internal;");
    let child = parse("pub fn helper() {}");
    let files = vec![
        ("src/application/mod.rs", &parent),
        ("src/application/internal.rs", &child),
    ];
    let aliases = aliases_from_files(&files);
    let by_layer = collect_pub_fns_by_layer(
        &files,
        &aliases,
        &three_layer(),
        &HashSet::new(),
        &HashSet::new(),
    );
    let app = names_for_layer(&by_layer, "application");
    assert!(
        !app.contains("helper"),
        "pub fn in a file-backed private module must not enter the target-layer surface, got {app:?}"
    );
}

#[test]
fn test_collect_pub_fns_includes_pub_fn_in_file_backed_public_module() {
    // Sanity counterpart: `pub mod internal;` makes the file public,
    // so `pub fn helper()` IS recorded.
    let parent = parse("pub mod internal;");
    let child = parse("pub fn helper() {}");
    let files = vec![
        ("src/application/mod.rs", &parent),
        ("src/application/internal.rs", &child),
    ];
    let aliases = aliases_from_files(&files);
    let by_layer = collect_pub_fns_by_layer(
        &files,
        &aliases,
        &three_layer(),
        &HashSet::new(),
        &HashSet::new(),
    );
    let app = names_for_layer(&by_layer, "application");
    assert!(
        app.contains("helper"),
        "pub fn in a file-backed public module must be recorded, got {app:?}"
    );
}

#[test]
fn test_collect_pub_fns_skips_impl_methods_under_short_name_collision() {
    // Two distinct types named `Session` (one public, one in a
    // private inline mod) must NOT collide. The canonical-path-based
    // `visible_canonicals` set keys on full paths, so
    // `crate::cli::handlers::api::Session` and
    // `crate::cli::handlers::internal::Session` are distinct entries
    // — only the former is recorded (private mod's recursion is
    // skipped during the collection pass).
    let file = parse(
        r#"
        pub mod api {
            pub struct Session;
            impl Session {
                pub fn run(&self) {}
            }
        }
        mod internal {
            pub struct Session;
            impl Session {
                pub fn cleanup(&self) {}
            }
        }
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("run"),
        "public-mod impl method must be recorded, got {cli:?}"
    );
    assert!(
        !cli.contains("cleanup"),
        "private-mod impl method must not leak via short-name collision, got {cli:?}"
    );
}

// ── Deprecated-attribute detection (v1.2.1) ────────────────────────

fn deprecated_for_layer<'ast>(
    by_layer: &std::collections::HashMap<String, Vec<PubFnInfo<'ast>>>,
    layer: &str,
) -> HashMap<String, bool> {
    by_layer
        .get(layer)
        .map(|fns| {
            fns.iter()
                .map(|f| (f.fn_name.clone(), f.deprecated))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn pub_fn_records_deprecated_attribute_bare() {
    let file = parse(
        r#"
        #[deprecated]
        pub fn cmd_old() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    let dep = deprecated_for_layer(&by_layer, "cli");
    assert_eq!(dep.get("cmd_old"), Some(&true));
}

#[test]
fn pub_fn_records_deprecated_with_message() {
    let file = parse(
        r#"
        #[deprecated = "use cmd_new instead"]
        pub fn cmd_old() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    assert_eq!(
        deprecated_for_layer(&by_layer, "cli").get("cmd_old"),
        Some(&true)
    );
}

#[test]
fn pub_fn_records_deprecated_with_args() {
    let file = parse(
        r#"
        #[deprecated(since = "1.0", note = "use cmd_new")]
        pub fn cmd_old() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    assert_eq!(
        deprecated_for_layer(&by_layer, "cli").get("cmd_old"),
        Some(&true)
    );
}

#[test]
fn pub_fn_no_attribute_not_deprecated() {
    let file = parse(
        r#"
        pub fn cmd_active() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    assert_eq!(
        deprecated_for_layer(&by_layer, "cli").get("cmd_active"),
        Some(&false)
    );
}

#[test]
fn pub_fn_other_attribute_not_deprecated() {
    // `#[allow(unused)]` must NOT be misidentified as deprecation.
    let file = parse(
        r#"
        #[allow(unused)]
        pub fn cmd_active() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = {
        let aliases = aliases_from_files(&files);
        collect_pub_fns_by_layer(
            &files,
            &aliases,
            &three_layer(),
            &HashSet::new(),
            &HashSet::new(),
        )
    };
    assert_eq!(
        deprecated_for_layer(&by_layer, "cli").get("cmd_active"),
        Some(&false)
    );
}
