use super::*;

#[test]
fn test_collect_async_block() {
    let calls = calls_in(
        r#"
        use crate::app::session::RlmSession;
        pub fn cmd_search() {
            let s = RlmSession::open();
            let _fut = async { s.search(1) };
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(
        calls.contains("crate::app::session::RlmSession::search"),
        "calls = {:?}",
        calls
    );
}

#[test]
fn test_empty_body_yields_no_calls() {
    let calls = calls_in("pub fn f() {}", "src/cli/handlers.rs", "f");
    assert_eq!(calls, HashSet::<String>::new());
}

#[test]
fn test_local_helper_call_resolves_to_crate_module() {
    // Regression: `helper()` without a `use` statement is a valid Rust
    // same-module call. Must resolve to `crate::<file_module>::helper`
    // so the graph sees the edge — not `<bare>:helper` dead-end.
    let calls = calls_in(
        r#"
        fn helper() {}
        pub fn cmd_foo() {
            helper();
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_foo",
    );
    assert!(
        calls.contains("crate::cli::handlers::helper"),
        "local helper must resolve via file module, got {calls:?}"
    );
    assert!(
        !calls.contains("<bare>:helper"),
        "local helper must not fall back to bare, got {calls:?}"
    );
}

#[test]
fn test_external_call_without_use_still_falls_to_bare() {
    // Conservative: if the first segment isn't in local_symbols (and no
    // `use` aliased it), stay `<bare>:…`. Otherwise external crate or
    // stdlib calls would be wrongly attributed to the local module.
    let calls = calls_in(
        r#"
        pub fn cmd_foo() {
            not_a_local_symbol();
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_foo",
    );
    assert!(
        calls.contains("<bare>:not_a_local_symbol"),
        "unknown fn must stay bare, got {calls:?}"
    );
}

#[test]
fn test_super_aliased_call_normalises_to_crate_rooted() {
    // `use super::stats::get_stats;` expands to `["super","stats","get_stats"]`
    // in the alias map. Without normalisation the canonical would be
    // `super::stats::get_stats`, which never matches graph nodes.
    // Post-alias re-normalisation turns it into `crate::…::get_stats`.
    let calls = calls_in(
        r#"
        use super::stats::get_stats;
        pub fn cmd_foo() {
            get_stats();
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_foo",
    );
    assert!(
        calls.contains("crate::cli::stats::get_stats"),
        "super-aliased call must normalise to crate::, got {calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("super::")),
        "super-rooted canonical must not leak, got {calls:?}"
    );
}

#[test]
fn test_unqualified_local_type_in_signature_resolves() {
    // `struct Session;` declared in this file + `fn f(s: Session)` — Rust
    // doesn't require a `use` for same-file types. Receiver tracking must
    // still resolve `s.search()` via the local-type fallback.
    let calls = calls_in(
        r#"
        pub struct Session;
        impl Session {
            pub fn search(&self) {}
        }
        pub fn cmd_foo(s: Session) {
            s.search();
        }
        "#,
        "src/application/session.rs",
        "cmd_foo",
    );
    assert!(
        calls.contains("crate::application::session::Session::search"),
        "unqualified local-type receiver must resolve, got {calls:?}"
    );
    assert!(
        !calls.contains("<method>:search"),
        "must not fall back to <method>:, got {calls:?}"
    );
}

#[test]
fn test_rust2018_absolute_call_without_use_resolves_to_crate_rooted() {
    // Regression: `app::foo()` called directly (no `use app::foo;`) is
    // also a crate-root module call in Rust 2018+. Must resolve to
    // `crate::app::foo`, mirroring the alias-backed case.
    let calls = calls_in_roots(
        r#"
        pub fn cmd_x() {
            app::foo();
        }
        "#,
        &["app"],
        "src/cli/handlers.rs",
        "cmd_x",
    );
    assert!(
        calls.contains("crate::app::foo"),
        "unaliased Rust 2018+ call must crate-prefix, got {calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c == "<bare>:app::foo"),
        "must not fall back to bare, got {calls:?}"
    );
}

#[test]
fn test_rust2018_absolute_import_resolves_to_crate_rooted() {
    // Rust 2018+: `use app::foo;` at the top of a non-root file is the
    // crate-root module `app`, equivalent to `use crate::app::foo;`.
    // When `app` is a known workspace root module, the alias expansion
    // must prepend `crate::` so the call graph matches.
    let calls = calls_in_roots(
        r#"
        use app::foo;
        pub fn cmd_x() {
            foo();
        }
        "#,
        &["app"],
        "src/cli/handlers.rs",
        "cmd_x",
    );
    assert!(
        calls.contains("crate::app::foo"),
        "Rust 2018+ absolute import must normalise to crate::, got {calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c == "app::foo"),
        "must not leave unprefixed app::foo, got {calls:?}"
    );
}

#[test]
fn test_collect_crate_root_modules_from_paths() {
    // `src/app/mod.rs`, `src/app/session.rs`, `src/cli/handlers.rs` →
    // {"app", "cli"}. `src/lib.rs` and `src/main.rs` are excluded.
    let roots = roots_from_paths(&[
        "src/app/mod.rs",
        "src/app/session.rs",
        "src/cli/handlers.rs",
        "src/lib.rs",
        "src/main.rs",
    ]);
    assert!(roots.contains("app"));
    assert!(roots.contains("cli"));
    assert!(!roots.contains("lib"));
    assert!(!roots.contains("main"));
}

#[test]
fn test_top_level_self_as_alias_maps_to_current_file() {
    // `use self as fs;` at the top of `src/util/fs_helpers.rs` — `self`
    // at crate-root-adjacent position means the current file's module.
    // Downstream normalisation must resolve `fs::something` to
    // `crate::util::fs_helpers::something`, not leak as a dead-end.
    let calls = calls_in(
        r#"
        use self as fs;
        pub fn cmd_x() {
            fs::something();
        }
        "#,
        "src/util/fs_helpers.rs",
        "cmd_x",
    );
    assert!(
        calls.contains("crate::util::fs_helpers::something"),
        "top-level self-alias must resolve to the current file's module, got {calls:?}"
    );
}

#[test]
fn test_qualified_impl_path_does_not_double_crate() {
    // `impl crate::app::Session { fn search() }` — the impl header
    // already gives a crate-rooted path. The canonical Self-target must
    // be `crate::app::Session::search`, NOT
    // `crate::<file_module>::crate::app::Session::search`.
    let fctx = load(
        r#"
        impl crate::app::Session {
            pub fn search(&self) {
                Self::internal_helper();
            }
        }
        "#,
    );
    let (item, f) = find_impl_fn(&fctx.file, "Session", "search");
    let self_ty = canonical_of_impl_self(item);
    let ctx = FnContext {
        file: &FileScope {
            path: "src/other_file.rs",
            alias_map: &fctx.alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &fctx.local_symbols,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &fctx.crate_root_modules,
            workspace_module_paths: None,
        },
        mod_stack: &[],
        body: &f.block,
        signature_params: sig_params(&f.sig),
        generic_params: std::collections::HashMap::new(),
        self_type: self_ty,
        workspace_index: None,
        workspace_files: None,
        reexports: None,
    };
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::app::Session::internal_helper"),
        "qualified impl path must canonicalise as-is, got {calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.contains("crate::crate::")),
        "must not double-crate, got {calls:?}"
    );
}
