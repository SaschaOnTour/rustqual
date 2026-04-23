//! Tests for the canonical call-target collector — the pre-pass that
//! turns a `syn::Block` into a `HashSet<String>` of canonical targets,
//! including receiver-type-tracked method calls.

use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
    collect_canonical_calls, FnContext,
};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::collect_local_symbols;
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
}

fn load(src: &str) -> FileCtx {
    let file = parse_file(src);
    let alias_map = gather_alias_map(&file);
    let local_symbols = collect_local_symbols(&file);
    FileCtx {
        file,
        alias_map,
        local_symbols,
    }
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
        importing_file,
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
        importing_file: "src/application/session.rs",
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
        importing_file: "src/other_file.rs",
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
