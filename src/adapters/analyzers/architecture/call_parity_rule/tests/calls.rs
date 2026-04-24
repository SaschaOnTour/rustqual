//! Tests for the canonical call-target collector — the pre-pass that
//! turns a `syn::Block` into a `HashSet<String>` of canonical targets,
//! including receiver-type-tracked method calls.

use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
    collect_canonical_calls, FnContext,
};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
    collect_crate_root_modules, collect_local_symbols,
};
use crate::adapters::shared::use_tree::gather_alias_map;
use std::collections::{HashMap, HashSet};

fn parse_file(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse file")
}

fn parse_type(src: &str) -> syn::Type {
    syn::parse_str(src).expect("parse type")
}

/// Build a context from a full file source plus the name of the fn whose
/// body we want to analyse. Picks up the fn's signature + alias map
/// automatically.
struct FileCtx {
    file: syn::File,
    alias_map: HashMap<String, Vec<String>>,
    local_symbols: HashSet<String>,
    crate_root_modules: HashSet<String>,
}

fn load(src: &str) -> FileCtx {
    let file = parse_file(src);
    let alias_map = gather_alias_map(&file);
    let local_symbols = collect_local_symbols(&file);
    // For single-file unit tests the root module set is empty — tests
    // that exercise Rust-2018 absolute imports populate it manually.
    let crate_root_modules = HashSet::new();
    FileCtx {
        file,
        alias_map,
        local_symbols,
        crate_root_modules,
    }
}

fn load_with_roots(src: &str, roots: &[&str]) -> FileCtx {
    let mut fctx = load(src);
    fctx.crate_root_modules = roots.iter().map(|s| s.to_string()).collect();
    fctx
}

/// Convenience — rebuild crate_root_modules from a slice of pseudo-file
/// paths (same shape `build_call_graph` sees) so tests match the real
/// pipeline's derivation.
fn roots_from_paths(paths: &[&str]) -> HashSet<String> {
    let fake: Vec<(&str, &syn::File)> = Vec::new();
    let _ = fake;
    let dummy = parse_file("");
    let refs: Vec<(&str, &syn::File)> = paths.iter().map(|p| (*p, &dummy)).collect();
    collect_crate_root_modules(&refs)
}

fn find_fn<'a>(file: &'a syn::File, name: &str) -> &'a syn::ItemFn {
    file.items
        .iter()
        .find_map(|i| match i {
            syn::Item::Fn(f) if f.sig.ident == name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fn {name} not found"))
}

fn impl_self_ty_name(item_impl: &syn::ItemImpl) -> Option<String> {
    match item_impl.self_ty.as_ref() {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

fn find_impl_fn<'a>(
    file: &'a syn::File,
    type_name: &str,
    fn_name: &str,
) -> (&'a syn::ItemImpl, &'a syn::ImplItemFn) {
    file.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Impl(i) if impl_self_ty_name(i).as_deref() == Some(type_name) => Some(i),
            _ => None,
        })
        .find_map(|item_impl| {
            item_impl.items.iter().find_map(|it| match it {
                syn::ImplItem::Fn(f) if f.sig.ident == fn_name => Some((item_impl, f)),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("impl {type_name}::{fn_name} not found"))
}

fn sig_params(sig: &syn::Signature) -> Vec<(String, &syn::Type)> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pt) => {
                let name = match pt.pat.as_ref() {
                    syn::Pat::Ident(pi) => pi.ident.to_string(),
                    _ => return None,
                };
                Some((name, pt.ty.as_ref()))
            }
            _ => None,
        })
        .collect()
}

fn ctx_for_fn<'a>(fctx: &'a FileCtx, fn_name: &str, importing_file: &'a str) -> FnContext<'a> {
    let f = find_fn(&fctx.file, fn_name);
    FnContext {
        body: &f.block,
        signature_params: sig_params(&f.sig),
        self_type: None,
        alias_map: &fctx.alias_map,
        local_symbols: &fctx.local_symbols,
        crate_root_modules: &fctx.crate_root_modules,
        importing_file,
        workspace_index: None,
    }
}

fn canonical_of_impl_self(item: &syn::ItemImpl) -> Option<Vec<String>> {
    if let syn::Type::Path(p) = item.self_ty.as_ref() {
        Some(
            p.path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect(),
        )
    } else {
        None
    }
}

// ── Basic call resolution ─────────────────────────────────────

#[test]
fn test_collect_direct_qualified_call() {
    let fctx = load(
        r#"
        pub fn cmd_search() {
            crate::application::stats::get_stats(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::application::stats::get_stats"));
}

#[test]
fn test_collect_unqualified_via_use_alias() {
    let fctx = load(
        r#"
        use crate::application::stats::get_stats;
        pub fn cmd_search() {
            get_stats(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::application::stats::get_stats"));
}

#[test]
fn test_collect_unqualified_no_alias_is_bare() {
    let fctx = load(
        r#"
        pub fn cmd_search() {
            foo(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("<bare>:foo"));
}

#[test]
fn test_collect_in_semicolon_separated_macro_descends() {
    // Regression: `vec![expr; n]`-style macros have tokens `expr ; n`
    // which fails the comma-list parser. The block fallback wraps them
    // in braces and extracts stmt-level expressions.
    let fctx = load(
        r#"
        pub fn cmd_search() {
            let _v = vec![compute(x); 3];
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("<bare>:compute"),
        "`;`-separated macro body must still descend, got {calls:?}"
    );
}

#[test]
fn test_collect_in_macro_descends() {
    let fctx = load(
        r#"
        pub fn cmd_search() {
            debug_assert!(validate(1));
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("<bare>:validate"));
    // The macro itself must not be recorded as a call target.
    assert!(!calls.contains("<macro>:debug_assert"));
}

#[test]
fn test_collect_self_super_prefix() {
    // `self::` inside `src/cli/mod.rs` resolves against module `crate::cli`,
    // so `self::helpers::format` → `crate::cli::helpers::format`. A non-mod
    // file (e.g. `src/cli/handlers.rs`) would resolve to
    // `crate::cli::handlers::helpers::format`, which is the Rust-semantic
    // outcome; the collector defers to `resolve_to_crate_absolute` for
    // this so both cases stay consistent with the file's module path.
    let fctx = load(
        r#"
        pub fn cmd_search() {
            self::helpers::format(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/mod.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::cli::helpers::format"),
        "calls = {:?}",
        calls
    );
}

#[test]
fn test_collect_turbofish_stripped() {
    let fctx = load(
        r#"
        pub fn cmd_search() {
            Box::<u32>::new(42);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("<bare>:Box::new"));
}

#[test]
fn test_collect_closure_body_collected() {
    let fctx = load(
        r#"
        pub fn cmd_search() {
            let f = |x: u32| inner_call(x);
            f(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("<bare>:inner_call"));
}

#[test]
fn test_collect_await_is_not_extra_call() {
    let fctx = load(
        r#"
        pub async fn cmd_search() {
            f(1).await;
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("<bare>:f"));
    // .await is not a call target
    assert!(!calls.iter().any(|c| c.contains("await")));
}

#[test]
fn test_collect_self_dispatch_in_impl() {
    let fctx = load(
        r#"
        pub struct RlmSession;
        impl RlmSession {
            pub fn search(&self) {
                Self::internal_helper();
            }
        }
        "#,
    );
    let (item, f) = find_impl_fn(&fctx.file, "RlmSession", "search");
    let self_ty = canonical_of_impl_self(item);
    let ctx = FnContext {
        body: &f.block,
        signature_params: sig_params(&f.sig),
        self_type: self_ty,
        alias_map: &fctx.alias_map,
        local_symbols: &fctx.local_symbols,
        crate_root_modules: &fctx.crate_root_modules,
        importing_file: "src/application/session.rs",
        workspace_index: None,
    };
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::application::session::RlmSession::internal_helper"),
        "calls = {:?}",
        calls
    );
}

// ── Receiver-Type-Tracking ────────────────────────────────────

#[test]
fn test_tracker_let_constructor_binding() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub fn cmd_search(q: u32) {
            let s = RlmSession::open_cwd();
            s.search(q);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::app::session::RlmSession::search"),
        "calls = {:?}",
        calls
    );
}

#[test]
fn test_tracker_let_type_annotation() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub fn cmd_search(q: u32) {
            let s: RlmSession = make_session();
            s.search(q);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::app::session::RlmSession::search"));
}

#[test]
fn test_tracker_fn_param_type() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub fn handle(session: RlmSession) {
            session.search(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "handle", "src/mcp/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::app::session::RlmSession::search"));
}

#[test]
fn test_tracker_fn_param_ref_type() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub fn handle(session: &RlmSession) {
            session.search(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "handle", "src/mcp/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::app::session::RlmSession::search"));
}

#[test]
fn test_tracker_fn_param_arc_type() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        use std::sync::Arc;
        pub fn handle(session: Arc<RlmSession>) {
            session.search(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "handle", "src/mcp/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::app::session::RlmSession::search"));
}

#[test]
fn test_tracker_fn_param_box_ref_mut_type() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub fn a(session: Box<RlmSession>) { session.search(1); }
        pub fn b(session: &mut RlmSession) { session.search(1); }
        "#,
    );
    for name in &["a", "b"] {
        let ctx = ctx_for_fn(&fctx, name, "src/mcp/handlers.rs");
        let calls = collect_canonical_calls(&ctx);
        assert!(
            calls.contains("crate::app::session::RlmSession::search"),
            "fn {name} calls = {:?}",
            calls
        );
    }
}

#[test]
fn test_tracker_alias_resolved_constructor() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub fn cmd_search() {
            let s = RlmSession::open();
            s.search(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::app::session::RlmSession::search"));
}

#[test]
fn test_tracker_shadowing_uses_latest() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        use crate::cli::CliSession;
        pub fn cmd_search() {
            let s = CliSession::new();
            let s = RlmSession::open();
            s.search(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::app::session::RlmSession::search"));
    assert!(!calls.contains("crate::cli::CliSession::search"));
}

#[test]
fn test_tracker_unknown_receiver_falls_back_to_method_shape() {
    let fctx = load(
        r#"
        pub fn cmd_search(x: UnknownType) {
            x.search(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("<method>:search"));
    assert!(!calls.iter().any(|c| c.contains("UnknownType::search")));
}

#[test]
fn test_tracker_closure_inherits_parent_bindings() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub fn cmd_search() {
            let s = RlmSession::open();
            let f = || s.search(1);
            f();
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::app::session::RlmSession::search"));
}

#[test]
fn test_tracker_factory_helper_unresolved_falls_back_to_method_shape() {
    // Documented limitation: no 1-hop return-type inference.
    let fctx = load(
        r#"
        pub fn cmd_search() {
            let s = helpers::open_session();
            s.search(1);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("<method>:search"));
}

#[test]
fn test_tracker_in_async_fn() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub async fn handle(s: RlmSession) {
            s.search(1).await;
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "handle", "src/mcp/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::app::session::RlmSession::search"));
}

#[test]
fn test_collect_async_block() {
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub fn cmd_search() {
            let s = RlmSession::open();
            let _fut = async { s.search(1) };
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::app::session::RlmSession::search"),
        "calls = {:?}",
        calls
    );
}

#[test]
fn test_empty_body_yields_no_calls() {
    let fctx = load("pub fn f() {}");
    let ctx = ctx_for_fn(&fctx, "f", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert_eq!(calls, HashSet::<String>::new());
}

#[test]
fn test_local_helper_call_resolves_to_crate_module() {
    // Regression: `helper()` without a `use` statement is a valid Rust
    // same-module call. Must resolve to `crate::<file_module>::helper`
    // so the graph sees the edge — not `<bare>:helper` dead-end.
    let fctx = load(
        r#"
        fn helper() {}
        pub fn cmd_foo() {
            helper();
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_foo", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
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
    let fctx = load(
        r#"
        pub fn cmd_foo() {
            not_a_local_symbol();
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_foo", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
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
    let fctx = load(
        r#"
        use super::stats::get_stats;
        pub fn cmd_foo() {
            get_stats();
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_foo", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
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
    let fctx = load(
        r#"
        pub struct Session;
        impl Session {
            pub fn search(&self) {}
        }
        pub fn cmd_foo(s: Session) {
            s.search();
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_foo", "src/application/session.rs");
    let calls = collect_canonical_calls(&ctx);
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
    let fctx = load_with_roots(
        r#"
        pub fn cmd_x() {
            app::foo();
        }
        "#,
        &["app"],
    );
    let ctx = ctx_for_fn(&fctx, "cmd_x", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
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
    let fctx = load_with_roots(
        r#"
        use app::foo;
        pub fn cmd_x() {
            foo();
        }
        "#,
        &["app"],
    );
    let ctx = ctx_for_fn(&fctx, "cmd_x", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
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
    let fctx = load(
        r#"
        use self as fs;
        pub fn cmd_x() {
            fs::something();
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_x", "src/util/fs_helpers.rs");
    let calls = collect_canonical_calls(&ctx);
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
        body: &f.block,
        signature_params: sig_params(&f.sig),
        self_type: self_ty,
        alias_map: &fctx.alias_map,
        local_symbols: &fctx.local_symbols,
        crate_root_modules: &fctx.crate_root_modules,
        importing_file: "src/other_file.rs",
        workspace_index: None,
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

// ── Shallow-inference fallback (Task 1.6) ────────────────────────

/// Helper: build a `FnContext` with a pre-populated workspace index.
fn ctx_with_index<'a>(
    fctx: &'a FileCtx,
    fn_name: &str,
    importing_file: &'a str,
    index: &'a crate::adapters::analyzers::architecture::call_parity_rule::type_infer::WorkspaceTypeIndex,
) -> FnContext<'a> {
    let f = find_fn(&fctx.file, fn_name);
    FnContext {
        body: &f.block,
        signature_params: sig_params(&f.sig),
        self_type: None,
        alias_map: &fctx.alias_map,
        local_symbols: &fctx.local_symbols,
        crate_root_modules: &fctx.crate_root_modules,
        importing_file,
        workspace_index: Some(index),
    }
}

#[test]
fn test_inference_fallback_resolves_rlm_pattern() {
    // The exact pattern that motivated Task 1.6: method chain on a
    // constructor + `?` unwrap. Legacy extract_let_binding can't see
    // through the MethodCall; inference walks the chain and ends at T.
    use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
        CanonicalType, WorkspaceTypeIndex,
    };
    let fctx = load(
        r#"
        use crate::app::session::Session;
        pub fn cmd_diff() {
            let session = Session::open().map_err(handle_err).unwrap();
            session.diff();
        }
        "#,
    );
    let mut index = WorkspaceTypeIndex::new();
    // `Session::open()` returns Result<Session, _>.
    index.method_returns.insert(
        (
            "crate::app::session::Session".to_string(),
            "open".to_string(),
        ),
        CanonicalType::Result(Box::new(CanonicalType::path([
            "crate", "app", "session", "Session",
        ]))),
    );
    let ctx = ctx_with_index(&fctx, "cmd_diff", "src/cli/handlers.rs", &index);
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "inference fallback should resolve session.diff(), got {calls:?}"
    );
}

#[test]
fn test_inference_fallback_resolves_field_access() {
    // `ctx.session.diff()` — receiver is Expr::Field, resolved via the
    // inference layer + workspace struct-field index. Fixture uses
    // `use crate::app::Ctx` so the signature-param `&Ctx` canonicalises
    // to `crate::app::Ctx` directly.
    use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
        CanonicalType, WorkspaceTypeIndex,
    };
    let fctx = load(
        r#"
        use crate::app::Ctx;
        pub fn handle_diff(ctx: &Ctx) {
            ctx.session.diff();
        }
        "#,
    );
    let mut index = WorkspaceTypeIndex::new();
    index.struct_fields.insert(
        ("crate::app::Ctx".to_string(), "session".to_string()),
        CanonicalType::path(["crate", "app", "Session"]),
    );
    let ctx = ctx_with_index(&fctx, "handle_diff", "src/cli/handlers.rs", &index);
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::app::Session::diff"),
        "field-access inference should resolve ctx.session.diff(), got {calls:?}"
    );
}

#[test]
fn test_inference_fallback_on_result_unwrap_chain() {
    // End-to-end: `session.open().unwrap().diff()` — combinator table
    // unwraps Result, then method resolution on Session proceeds.
    use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
        CanonicalType, WorkspaceTypeIndex,
    };
    let fctx = load(
        r#"
        use crate::app::session::Session;
        pub fn cmd_direct() {
            Session::open().unwrap().diff();
        }
        "#,
    );
    let mut index = WorkspaceTypeIndex::new();
    index.method_returns.insert(
        (
            "crate::app::session::Session".to_string(),
            "open".to_string(),
        ),
        CanonicalType::Result(Box::new(CanonicalType::path([
            "crate", "app", "session", "Session",
        ]))),
    );
    let ctx = ctx_with_index(&fctx, "cmd_direct", "src/cli/handlers.rs", &index);
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "combinator chain should resolve Session::open().unwrap().diff(), got {calls:?}"
    );
}

#[test]
fn test_existing_fast_path_still_works_without_index() {
    // Regression guard: legacy extract_let_binding keeps working when
    // workspace_index is None (unit-test fixture shape).
    let fctx = load(
        r#"
        use crate::app::session::RlmSession;
        pub fn cmd_search(q: u32) {
            let s = RlmSession::open_cwd();
            s.search(q);
        }
        "#,
    );
    let ctx = ctx_for_fn(&fctx, "cmd_search", "src/cli/handlers.rs");
    let calls = collect_canonical_calls(&ctx);
    assert!(calls.contains("crate::app::session::RlmSession::search"));
}
