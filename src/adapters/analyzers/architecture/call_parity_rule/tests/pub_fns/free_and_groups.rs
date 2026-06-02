use super::*;

#[test]
fn test_collect_pub_fns_in_layer_free_fn() {
    let file = parse(
        r#"
        pub fn cmd_stats() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(cli.contains("cmd_stats"), "cli = {cli:?}");
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
    let by_layer = pub_fns_by_layer(&files);
    let cli = names_for_layer(&by_layer, "cli");
    assert!(cli.contains("cmd_stats"), "pub(crate) must be collected");
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
    let by_layer = pub_fns_by_layer(&files);
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
    let by_layer = pub_fns_by_layer(&files);
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
    let by_layer = collect_pub_fns_by_layer(PubFnInputs {
        files: &files,
        aliases_per_file: &aliases,
        layers: &three_layer(),
        transparent_wrappers: &HashSet::new(),
        promoted_attributes: &HashSet::new(),
        workspace: &crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::WorkspaceLookup {
            cfg_test_files: &cfg_test,
            crate_root_modules: &HashSet::new(),
            workspace_module_paths: &HashSet::new(),
        },
    });
    assert!(
        names_for_layer(&by_layer, "cli").is_empty(),
        "cfg-test file must be skipped wholesale"
    );
}

#[test]
fn test_collect_pub_fns_skips_unmatched_files() {
    // File not covered by any layer — its pub fns are not part of the
    // call-parity scope (neither as adapter member nor as target).
    let file = parse("pub fn free_floating() {}");
    let files = vec![("src/utils/misc.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    for layer in ["application", "cli", "mcp"] {
        assert!(
            names_for_layer(&by_layer, layer).is_empty(),
            "layer {layer} must be empty"
        );
    }
}
