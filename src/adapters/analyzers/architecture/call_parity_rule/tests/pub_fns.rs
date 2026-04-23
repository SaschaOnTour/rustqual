//! Tests for `collect_pub_fns_by_layer` — workspace-wide pub-fn
//! enumeration grouped by architecture layer.

use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::{
    collect_pub_fns_by_layer, PubFnInfo,
};
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashSet;

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
    let files = vec![("src/cli/handlers.rs".to_string(), "".to_string(), &file)];
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &HashSet::new());
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
    let files = vec![("src/cli/handlers.rs".to_string(), "".to_string(), &file)];
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &HashSet::new());
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
    let files = vec![("src/cli/handlers.rs".to_string(), "".to_string(), &file)];
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &HashSet::new());
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
    let files = vec![("src/cli/handlers.rs".to_string(), "".to_string(), &file)];
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &HashSet::new());
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
    let files = vec![(
        "src/application/session.rs".to_string(),
        "".to_string(),
        &file,
    )];
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &HashSet::new());
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
        (
            "src/application/session.rs".to_string(),
            "".to_string(),
            &decl_file,
        ),
        (
            "src/application/session_impls.rs".to_string(),
            "".to_string(),
            &impl_file,
        ),
    ];
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &HashSet::new());
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
    let files = vec![(
        "src/application/session.rs".to_string(),
        "".to_string(),
        &file,
    )];
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &HashSet::new());
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
        ("src/cli/handlers.rs".to_string(), "".to_string(), &cli_file),
        ("src/mcp/handlers.rs".to_string(), "".to_string(), &mcp_file),
        (
            "src/application/stats.rs".to_string(),
            "".to_string(),
            &app_file,
        ),
    ];
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &HashSet::new());
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
    let files = vec![("src/cli/handlers.rs".to_string(), "".to_string(), &file)];
    let mut cfg_test = HashSet::new();
    cfg_test.insert("src/cli/handlers.rs".to_string());
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &cfg_test);
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
    let files = vec![("src/cli/handlers.rs".to_string(), "".to_string(), &file)];
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &HashSet::new());
    let cli = names_for_layer(&by_layer, "cli");
    assert!(cli.contains("cmd_stats"));
    assert!(!cli.contains("not_a_handler"), "#[test] fn must be skipped");
}

#[test]
fn test_collect_pub_fns_skips_unmatched_files() {
    // File not covered by any layer — its pub fns are not part of the
    // call-parity scope (neither as adapter member nor as target).
    let file = parse("pub fn free_floating() {}");
    let files = vec![("src/utils/misc.rs".to_string(), "".to_string(), &file)];
    let by_layer = collect_pub_fns_by_layer(&files, &adapter_layers(), &HashSet::new());
    for layer in ["application", "cli", "mcp"] {
        assert!(
            names_for_layer(&by_layer, layer).is_empty(),
            "layer {layer} must be empty"
        );
    }
}
