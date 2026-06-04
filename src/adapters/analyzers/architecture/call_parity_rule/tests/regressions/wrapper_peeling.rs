use super::*;

#[test]
fn qualified_path_does_not_alias_promote_through_leaf() {
    // `use std::sync::Arc as Shared;` is in scope, but the use site
    // is `wrap::Shared<Session>` — a *qualified* path. The leaf
    // `Shared` matches the alias name, but the prefix `wrap::` makes
    // the type unrelated. Receiver inference must NOT peel
    // `wrap::Shared<Session>` to `Session::diff` just because the
    // bare-`Shared` alias resolves to `Arc`. (Session is in scope
    // here so alias-promotion would otherwise produce a real edge.)
    let fx = parse(
        r#"
        use std::sync::Arc as Shared;
        use crate::app::session::Session;
        pub fn handle(s: wrap::Shared<Session>) {
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "handle");
    assert!(
        !calls.contains("crate::app::session::Session::diff"),
        "qualified path must not be alias-promoted, got {calls:?}"
    );
}

#[test]
fn qualified_local_arc_does_not_auto_peel() {
    // `wrap::Arc<Session>` where `wrap::Arc` is a *local* type that
    // happens to be named `Arc`. Direct wrapper dispatch must NOT
    // peel just because the leaf is `Arc`. Only stdlib-rooted
    // qualifications (`std::sync::Arc`) auto-peel.
    let fx = parse(
        r#"
        use crate::app::session::Session;
        mod wrap { pub struct Arc<T>(T); }
        pub fn handle(s: wrap::Arc<Session>) {
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "handle");
    assert!(
        !calls.contains("crate::app::session::Session::diff"),
        "qualified local Arc must not auto-peel as stdlib Arc, got {calls:?}"
    );
}

#[test]
fn bare_local_arc_does_not_auto_peel() {
    // `use crate::wrap::Arc;` then `s: Arc<Session>` — `Arc` is
    // single-segment and matches the stdlib wrapper list, but the
    // active `use` resolves it to a *local* type. The bare-name
    // fast path must canonicalise first and skip auto-peeling for
    // non-stdlib targets.
    let fx = parse(
        r#"
        use crate::wrap::Arc;
        use crate::app::session::Session;
        pub fn handle(s: Arc<Session>) {
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "handle");
    assert!(
        !calls.contains("crate::app::session::Session::diff"),
        "bare Arc shadowed by local must not auto-peel, got {calls:?}"
    );
}

#[test]
fn user_transparent_wrapper_peels_to_inner_receiver() {
    // With `transparent_wrappers = ["State"]`, an external `State<Session>`
    // wrapper (whose `axum::*` path the canonicaliser can't resolve) must
    // peel via the leaf segment so `s.diff()` resolves to the inner Session
    // — whether the wrapper is fully qualified or a renamed import. (label, src)
    let cases: &[(&str, &str)] = &[
        (
            "fully-qualified axum::extract::State",
            r#"
            use crate::app::session::Session;
            pub fn handle(s: axum::extract::State<Session>) {
                s.diff();
            }
            "#,
        ),
        (
            "renamed State as ExtractState",
            r#"
            use axum::extract::State as ExtractState;
            use crate::app::session::Session;
            pub fn handle(s: ExtractState<Session>) {
                s.diff();
            }
            "#,
        ),
    ];
    for (label, src) in cases {
        let fx = parse(src);
        let mut index = sample_session_index();
        index.transparent_wrappers.insert("State".to_string());
        let calls = run(&fx, &index, "handle");
        assert!(
            calls.contains("crate::app::session::Session::diff"),
            "case {label}: user-transparent wrapper must peel; got {calls:?}"
        );
    }
}

#[test]
fn aliased_local_wrapper_does_not_auto_peel() {
    // `use crate::wrap::Arc as Shared;` aliases a *local* wrapper
    // type to `Shared`. The local `crate::wrap::Arc` may not be
    // Deref-transparent like stdlib Arc, so receiver inference must
    // NOT auto-peel `Shared<Session>` just because the alias's leaf
    // segment is `Arc`. Only when the alias canonical lives in
    // `std`/`core`/`alloc` do we trust the auto-peel.
    let fx = parse(
        r#"
        use crate::wrap::Arc as Shared;
        use crate::app::session::Session;
        pub fn handle(s: Shared<Session>) {
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "handle");
    assert!(
        !calls.contains("crate::app::session::Session::diff"),
        "aliased local wrapper must not auto-peel as stdlib Arc, got {calls:?}"
    );
}

#[test]
fn aliased_stdlib_wrapper_inside_inline_mod_peels_to_inner() {
    // Same renamed-Arc test, but the `use` statement lives inside an
    // inline mod. Top-level `alias_map` doesn't see it; the scoped
    // overlay does. Receiver resolution must consult the scoped
    // overlay for wrapper-name promotion.
    let fx = parse(
        r#"
        mod inner {
            use std::sync::Arc as Shared;
            use crate::app::session::Session;
            pub fn handle(s: Shared<Session>) {
                s.diff();
            }
        }
        "#,
    );
    let f = find_fn_in_mod(&fx.file, "inner", "handle");
    let ctx = FnContext {
        file: &FileScope {
            path: "src/cli/handlers.rs",
            alias_map: &fx.alias_map,
            aliases_per_scope: &gather_alias_map_scoped(&fx.file),
            local_symbols: &fx.local_symbols,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &fx.crate_roots,
            workspace_module_paths: None,
        },
        mod_stack: &["inner".to_string()],
        body: &f.block,
        signature_params: sig_params(&f.sig),
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&sample_session_index()),
        workspace_files: None,
        reexports: None,
    };
    let calls = collect_canonical_calls(&ctx);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "scoped Arc-alias inside inline mod must peel to Session, got {calls:?}"
    );
}

fn find_fn_in_mod<'a>(file: &'a syn::File, mod_name: &str, fn_name: &str) -> &'a syn::ItemFn {
    file.items
        .iter()
        .find_map(|item| match item {
            syn::Item::Mod(m) if m.ident == mod_name => m.content.as_ref(),
            _ => None,
        })
        .and_then(|(_, items)| {
            items.iter().find_map(|i| match i {
                syn::Item::Fn(f) if f.sig.ident == fn_name => Some(f),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("fn {mod_name}::{fn_name} not found"))
}

#[test]
fn aliased_stdlib_wrapper_peels_to_inner() {
    // `use std::sync::Arc as Shared;` then `fn h(s: Shared<Session>)`
    // — the receiver resolver must follow the alias to recognise
    // `Shared` as `Arc`, peel it, and reach `Session::diff`.
    let fx = parse(
        r#"
        use std::sync::Arc as Shared;
        use crate::app::session::Session;
        pub fn handle(s: Shared<Session>) {
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "handle");
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "aliased Arc wrapper must peel to Session, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Positive: free-fn return-type chain
// ═══════════════════════════════════════════════════════════════════

#[test]
fn free_fn_result_chain() {
    let fx = parse(
        r#"
        pub fn cmd() {
            let s = crate::app::make_session().unwrap();
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

// ═══════════════════════════════════════════════════════════════════
// Positive: fast-path patterns (no workspace_index needed, but still work)
// ═══════════════════════════════════════════════════════════════════
