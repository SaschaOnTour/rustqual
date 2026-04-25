//! Tests for `collect_pub_fns_by_layer` — workspace-wide pub-fn
//! enumeration grouped by architecture layer.

use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::{
    collect_pub_fns_by_layer, PubFnInfo,
};
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::shared::use_tree::gather_alias_map;
use globset::{Glob, GlobSet, GlobSetBuilder};
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

fn globset(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).unwrap());
    }
    b.build().unwrap()
}

fn adapter_layers() -> LayerDefinitions {
    LayerDefinitions::new(
        vec![
            "application".to_string(),
            "cli".to_string(),
            "mcp".to_string(),
        ],
        vec![
            ("application".to_string(), globset(&["src/application/**"])),
            ("cli".to_string(), globset(&["src/cli/**"])),
            ("mcp".to_string(), globset(&["src/mcp/**"])),
        ],
    )
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
    // fn search() }` in another — the workspace-wide visible-type set
    // must let the impl methods through, otherwise Check B misses
    // legitimate target-layer API.
    let decl_file = parse("pub struct Session;");
    let impl_file = parse(
        r#"
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
    let by_layer = collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &cfg_test);
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        !cli.contains("op"),
        "impl method on type in private mod must be skipped, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_records_pub_use_reexport_with_qualified_impl() {
    // `pub use private::Hidden;` with the impl at file level
    // (qualified `impl private::Hidden { … }`) — the re-export adds
    // `Hidden` to the workspace-visible-types set, and the impl
    // itself is OUTSIDE the private mod, so its methods get recorded.
    //
    // Limit (not covered): `impl Hidden { … }` *inside* `mod
    // private { … }` stays out — the enclosing-mod visibility gate
    // (which prevents short-name collisions with public siblings)
    // takes precedence. Document at call-site rather than special-
    // case the resolver. Workaround: lift the impl out of the
    // private mod or make the mod itself `pub`.
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
    };
    let cli = names_for_layer(&by_layer, "cli");
    assert!(
        cli.contains("op"),
        "re-exported type with file-level impl must be recorded, got {cli:?}"
    );
}

#[test]
fn test_collect_pub_fns_skips_impl_methods_under_short_name_collision() {
    // The visible-types set is keyed by short ident, so two distinct
    // types named `Session` (one public, one in a private inline mod)
    // collide. Without enclosing-mod gating on impl methods, the
    // private one's impl method would still register because the
    // short name "Session" lives in visible_types via the public
    // sibling. Gate on enclosing-mod visibility to keep the private
    // method out.
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
        collect_pub_fns_by_layer(&files, &aliases, &adapter_layers(), &HashSet::new())
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
