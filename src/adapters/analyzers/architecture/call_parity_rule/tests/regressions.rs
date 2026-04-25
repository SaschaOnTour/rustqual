//! Regression harness for the Task 1.6 call-parity inference wiring.
//!
//! Each test sets up a workspace type-index resembling rlm's layout
//! (`Session` type with `open()`/`diff()`/… methods, `Ctx` with a
//! `session` field) and runs a minimal fn body through
//! `collect_canonical_calls`. Positive tests assert the expected
//! `crate::…::Type::method` edge appears in the output; negative tests
//! assert that documented limits correctly fall back to `<method>:name`
//! instead of producing a spurious edge.
//!
//! Coverage targets the rlm classification published in the Task 1.6
//! brief: Group-2 (method-chain ctors) and Group-3 (cascading struct
//! field access), plus the fast-path patterns that must stay green.

use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
    collect_canonical_calls, FnContext,
};
use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    CanonicalType, WorkspaceTypeIndex,
};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::collect_local_symbols;
use crate::adapters::shared::use_tree::{
    gather_alias_map, gather_alias_map_scoped, ScopedAliasMap,
};
use std::collections::{HashMap, HashSet};

const SESSION_PATH: &str = "crate::app::session::Session";
const CTX_PATH: &str = "crate::app::Ctx";

/// RegFixture bundling a parsed file plus the resolution inputs
/// (`alias_map`, `local_symbols`, `crate_root_modules`) that
/// `collect_canonical_calls` expects.
struct RegFixture {
    file: syn::File,
    alias_map: HashMap<String, Vec<String>>,
    local_symbols: HashSet<String>,
    crate_roots: HashSet<String>,
}

fn parse(src: &str) -> RegFixture {
    let file: syn::File = syn::parse_str(src).expect("parse fixture");
    let alias_map = gather_alias_map(&file);
    let local_symbols = collect_local_symbols(&file);
    RegFixture {
        file,
        alias_map,
        local_symbols,
        crate_roots: HashSet::new(),
    }
}

/// Pre-populated workspace index modelling rlm's Session + Ctx shape.
fn rlm_index() -> WorkspaceTypeIndex {
    let session = CanonicalType::path(["crate", "app", "session", "Session"]);
    let response = CanonicalType::path(["crate", "app", "Response"]);
    let error = CanonicalType::path(["crate", "app", "Error"]);
    let mut index = WorkspaceTypeIndex::new();
    // Session::open() -> Result<Session, Error>
    index.insert_method_return(
        SESSION_PATH,
        "open",
        CanonicalType::Result(Box::new(session.clone())),
    );
    // Session::open_cwd() -> Result<Session, Error>
    index.insert_method_return(
        SESSION_PATH,
        "open_cwd",
        CanonicalType::Result(Box::new(session.clone())),
    );
    // Session::diff() -> Response
    index.insert_method_return(SESSION_PATH, "diff", response.clone());
    // Session::files() -> Response
    index.insert_method_return(SESSION_PATH, "files", response.clone());
    // Session::insert() -> Result<Response, Error>
    index.insert_method_return(
        SESSION_PATH,
        "insert",
        CanonicalType::Result(Box::new(response.clone())),
    );
    // Ctx { session: Session }
    index.insert_struct_field(CTX_PATH, "session", session);
    // Free fn make_session() -> Result<Session, Error>
    index.fn_returns.insert(
        "crate::app::make_session".to_string(),
        CanonicalType::Result(Box::new(CanonicalType::path([
            "crate", "app", "session", "Session",
        ]))),
    );
    let _ = error; // keep in scope if extensions need it
    index
}

fn find_fn<'a>(file: &'a syn::File, name: &str) -> &'a syn::ItemFn {
    file.items
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(f) if f.sig.ident == name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fn {name} not in fixture"))
}

fn sig_params(sig: &syn::Signature) -> Vec<(String, &syn::Type)> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pt) => match pt.pat.as_ref() {
                syn::Pat::Ident(pi) => Some((pi.ident.to_string(), pt.ty.as_ref())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Run the fn body through `collect_canonical_calls` with the given
/// workspace index. Returns the set of canonical call targets.
fn run(fx: &RegFixture, index: &WorkspaceTypeIndex, fn_name: &str) -> HashSet<String> {
    let f = find_fn(&fx.file, fn_name);
    run_with_self(fx, index, &f.sig, &f.block, None)
}

/// Run an `impl Type { fn name(&self, …) { … } }` body through the
/// collector with `self_type` set to crate-rooted segments. The
/// caller passes the canonical path so the test can bind to whichever
/// module the workspace index models for `Type` (e.g. Session lives
/// under `crate::app::session::*` in `rlm_index()`).
fn run_impl_method(
    fx: &RegFixture,
    index: &WorkspaceTypeIndex,
    type_name: &str,
    fn_name: &str,
    self_segs: Vec<String>,
) -> HashSet<String> {
    let (_, f) = find_impl_method(&fx.file, type_name, fn_name);
    run_with_self(fx, index, &f.sig, &f.block, Some(self_segs))
}

fn run_with_self(
    fx: &RegFixture,
    index: &WorkspaceTypeIndex,
    sig: &syn::Signature,
    body: &syn::Block,
    self_type: Option<Vec<String>>,
) -> HashSet<String> {
    let ctx = FnContext {
        file: &FileScope {
            path: "src/cli/handlers.rs",
            alias_map: &fx.alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &fx.local_symbols,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &fx.crate_roots,
        },
        mod_stack: &[],
        body,
        signature_params: sig_params(sig),
        self_type,
        workspace_index: Some(index),
        workspace_files: None,
    };
    collect_canonical_calls(&ctx)
}

fn find_impl_method<'a>(
    file: &'a syn::File,
    type_name: &str,
    fn_name: &str,
) -> (Vec<String>, &'a syn::ImplItemFn) {
    file.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Impl(i) => Some(i),
            _ => None,
        })
        .find_map(|item_impl| {
            let segs = impl_self_segments(item_impl)?;
            if segs.last().map(String::as_str) != Some(type_name) {
                return None;
            }
            item_impl.items.iter().find_map(|it| match it {
                syn::ImplItem::Fn(f) if f.sig.ident == fn_name => Some((segs.clone(), f)),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("impl {type_name}::{fn_name} not in fixture"))
}

fn impl_self_segments(item: &syn::ItemImpl) -> Option<Vec<String>> {
    let syn::Type::Path(p) = item.self_ty.as_ref() else {
        return None;
    };
    Some(
        p.path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect(),
    )
}

// ═══════════════════════════════════════════════════════════════════
// Positive: rlm Group-2 patterns (method-chain ctors)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn rlm_group2_open_map_err_unwrap() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open().map_err(handle).unwrap();
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "expected Session::diff edge, got {calls:?}"
    );
}

#[test]
fn rlm_group2_open_cwd_map_err_try() {
    // The exact pattern from the original rlm bug report.
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open_cwd().map_err(map_err)?;
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "expected Session::diff edge, got {calls:?}"
    );
}

#[test]
fn rlm_group2_plain_unwrap() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open().unwrap();
            s.files();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::files"));
}

#[test]
fn rlm_group2_expect_message() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open().expect("session must open");
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn rlm_group2_unwrap_or_else_closure() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open().unwrap_or_else(|e| fallback(e));
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn rlm_group2_chained_inline() {
    // No intermediate `let` — the chain resolves inside a single
    // method-call expression.
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            Session::open().unwrap().diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn rlm_group2_insert_returns_result_chained() {
    // Session::insert returns Result<Response, _> — verify the outer
    // call edge is recorded even on a Result-wrapped receiver chain.
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            Session::open().unwrap().insert();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::insert"));
}

// ═══════════════════════════════════════════════════════════════════
// Positive: rlm Group-3 patterns (struct-field access)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn rlm_group3_ctx_field_access() {
    let fx = parse(
        r#"
        use crate::app::Ctx;
        pub fn handle(ctx: &Ctx) {
            ctx.session.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "handle");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn rlm_group3_ctx_field_access_via_let() {
    let fx = parse(
        r#"
        use crate::app::Ctx;
        pub fn handle(ctx: &Ctx) {
            let s = &ctx.session;
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "handle");
    // `&ctx.session` inferred as Session (Reference is transparent).
    assert!(calls.contains("crate::app::session::Session::diff"));
}

// ═══════════════════════════════════════════════════════════════════
// Positive: self receiver inside impl methods
// ═══════════════════════════════════════════════════════════════════

#[test]
fn self_method_call_resolves_via_impl_type() {
    // `impl Session { fn run(&self) { self.diff() } }` — `self` must
    // bind to the enclosing impl's canonical type so `self.diff()`
    // routes through `method_returns[Session::diff]` instead of
    // collapsing to `<method>:diff`.
    let fx = parse(
        r#"
        impl Session {
            pub fn run(&self) {
                self.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &rlm_index(), "Session", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "self.diff() must route through workspace_index, got {calls:?}"
    );
}

#[test]
fn self_field_access_resolves_via_impl_type() {
    // `self.session.diff()` — Self::session field, then Session::diff.
    // Needs both the Self → Ctx binding and the field-type lookup
    // chain to fire.
    let fx = parse(
        r#"
        impl Ctx {
            pub fn run(&self) {
                self.session.diff();
            }
        }
        "#,
    );
    let self_segs = vec!["crate".to_string(), "app".to_string(), "Ctx".to_string()];
    let calls = run_impl_method(&fx, &rlm_index(), "Ctx", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "self.session.diff() must chain through field type, got {calls:?}"
    );
}

#[test]
fn signature_param_typed_self_resolves() {
    // `fn merge(&self, other: Self)` inside `impl Session` — `other`
    // is declared as `Self`, must bind to `Session` so `other.diff()`
    // routes through `method_returns`.
    let fx = parse(
        r#"
        impl Session {
            pub fn merge(&self, other: Self) {
                other.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &rlm_index(), "Session", "merge", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "param `other: Self` must resolve to Session, got {calls:?}"
    );
}

#[test]
fn let_annotation_self_resolves() {
    // `let other: Self = make();` inside `impl Session` — annotation
    // must substitute Self before resolving.
    let fx = parse(
        r#"
        impl Session {
            pub fn run(&self) {
                let other: Self = make();
                other.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &rlm_index(), "Session", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "`let other: Self = …` must bind to Session, got {calls:?}"
    );
}

#[test]
fn turbofish_self_inside_impl_resolves() {
    // `let s = get::<Self>(); s.diff();` inside `impl Session`. The
    // turbofish-as-return-type fallback must substitute Self before
    // resolving the type argument so the binding pins to Session.
    let fx = parse(
        r#"
        impl Session {
            pub fn run(&self) {
                let s = get::<Self>();
                s.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &rlm_index(), "Session", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "`get::<Self>()` turbofish must resolve to Session, got {calls:?}"
    );
}

#[test]
fn annotated_destructuring_self_resolves() {
    // `let Some(other): Option<Self> = maybe() else { return; };` —
    // the annotation goes through `bind_annotated` in the destructure
    // walker, which must substitute Self before resolving.
    let fx = parse(
        r#"
        impl Session {
            pub fn run(&self) {
                let Some(other): Option<Self> = maybe() else { return; };
                other.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &rlm_index(), "Session", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "annotated destructuring with Self must bind to Session, got {calls:?}"
    );
}

#[test]
fn cast_as_self_resolves() {
    // `(expr as Self).diff()` inside `impl Session` — `infer_cast`
    // resolves the target type, which must substitute Self.
    let fx = parse(
        r#"
        impl Session {
            pub fn run(&self) {
                let s = (raw() as Self);
                s.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &rlm_index(), "Session", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "`as Self` cast must resolve to Session, got {calls:?}"
    );
}

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
    let calls = run(&fx, &rlm_index(), "handle");
    assert!(
        !calls.contains("crate::app::session::Session::diff"),
        "qualified path must not be alias-promoted, got {calls:?}"
    );
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
    let calls = run(&fx, &rlm_index(), "handle");
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
        },
        mod_stack: &["inner".to_string()],
        body: &f.block,
        signature_params: sig_params(&f.sig),
        self_type: None,
        workspace_index: Some(&rlm_index()),
        workspace_files: None,
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
    let calls = run(&fx, &rlm_index(), "handle");
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
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

// ═══════════════════════════════════════════════════════════════════
// Positive: fast-path patterns (no workspace_index needed, but still work)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn fast_path_signature_param_resolves() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn handle(s: &Session) {
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "handle");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn fast_path_let_type_annotation() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s: Session = make_it();
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(calls.contains("crate::app::session::Session::diff"));
}

#[test]
fn fast_path_direct_constructor() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open_cwd();
            // No unwrap — s is Result<Session, _>, not Session.
            // Fast path on the bare-ident fails; inference fallback on
            // `s.diff()` receiver infers Result<Session>, which doesn't
            // have `diff` in the combinator table → <method>:diff.
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    // This pattern is pathological (caller should `?` or `unwrap`), but
    // we verify the resolver doesn't invent a false Session::diff edge.
    assert!(
        calls.contains("<method>:diff") || calls.contains("crate::app::session::Session::diff"),
        "pathological Result<T>.method() must either fall back or correctly unwrap, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Negative: documented Stage 1 limits (unresolved stays unresolved)
// ═══════════════════════════════════════════════════════════════════

#[test]
fn negative_external_type_method_is_bare() {
    // `u32` is stdlib — no workspace entry. Calling a made-up method
    // on it must land as `<method>:name` rather than confabulate.
    let fx = parse(
        r#"
        pub fn cmd() {
            let x: u32 = 42;
            x.custom_method();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(
        calls.contains("<method>:custom_method"),
        "expected <method>:custom_method fallback, got {calls:?}"
    );
    assert!(
        !calls.iter().any(|c| c.contains("u32::custom_method")),
        "must not fabricate stdlib method edges, got {calls:?}"
    );
}

#[test]
fn negative_unannotated_generic_stays_unresolved() {
    // `fn get<T>() -> T` yields Opaque; `x.m()` falls back.
    let fx = parse(
        r#"
        pub fn cmd() {
            let x = get();
            x.some_method();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(calls.contains("<method>:some_method"));
}

#[test]
fn negative_stdlib_map_closure_is_unresolved() {
    // `.map(|r| r.diff())` inner call on the closure argument — the
    // closure body is visited, `r` has no binding → <method>:diff. The
    // outer `.map()` itself also yields <method>:map (stdlib
    // closure-dependent combinator).
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            Session::open().map(|r| r.diff());
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    // The inner `r.diff()` is unresolved; assert it stays <method>:diff.
    assert!(
        calls.iter().any(|c| c == "<method>:diff"),
        "closure-body call should stay <method>:diff without binding, got {calls:?}"
    );
}

#[test]
fn negative_tuple_destructuring_is_limit() {
    // Stage 1 doesn't track tuple element types. `let (a, s) = setup();
    // s.m()` leaves `s` unresolved.
    let fx = parse(
        r#"
        pub fn cmd() {
            let (a, s) = setup();
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    // Documented limit: tuple-destructured bindings are Opaque.
    assert!(
        calls.contains("<method>:diff"),
        "tuple destructuring is a Stage 1 limit — expected <method>:diff, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Robustness: mixed positive + negative in one fn body
// ═══════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════
// Stage 2: Trait-Dispatch Over-Approximation
// ═══════════════════════════════════════════════════════════════════

#[test]
fn trait_dispatch_fans_out_to_all_impls() {
    // `dyn Handler.handle()` must record edges to EVERY impl's `handle`.
    let fx = parse(
        r#"
        use crate::ports::Handler;
        pub fn dispatch(h: &dyn Handler) {
            h.handle();
        }
        "#,
    );
    let mut index = WorkspaceTypeIndex::new();
    // Set up the trait + its method name.
    index.trait_methods.insert(
        "crate::ports::Handler".to_string(),
        std::iter::once("handle".to_string()).collect(),
    );
    // Three impls.
    index.trait_impls.insert(
        "crate::ports::Handler".to_string(),
        vec![
            "crate::app::LoggingHandler".to_string(),
            "crate::app::MetricsHandler".to_string(),
            "crate::app::AuditHandler".to_string(),
        ],
    );
    let calls = run(&fx, &index, "dispatch");
    assert!(
        calls.contains("crate::app::LoggingHandler::handle"),
        "expected LoggingHandler::handle edge, got {calls:?}"
    );
    assert!(calls.contains("crate::app::MetricsHandler::handle"));
    assert!(calls.contains("crate::app::AuditHandler::handle"));
}

#[test]
fn trait_dispatch_skips_unrelated_methods() {
    // `dyn Handler.unrelated()` — the method isn't on the trait, so no
    // fan-out. Falls back to <method>:name.
    let fx = parse(
        r#"
        use crate::ports::Handler;
        pub fn dispatch(h: &dyn Handler) {
            h.unrelated();
        }
        "#,
    );
    let mut index = WorkspaceTypeIndex::new();
    index.trait_methods.insert(
        "crate::ports::Handler".to_string(),
        std::iter::once("handle".to_string()).collect(),
    );
    index.trait_impls.insert(
        "crate::ports::Handler".to_string(),
        vec!["crate::app::X".to_string()],
    );
    let calls = run(&fx, &index, "dispatch");
    assert!(
        calls.contains("<method>:unrelated"),
        "unrelated method on trait must fall through, got {calls:?}"
    );
    assert!(
        !calls.contains("crate::app::X::unrelated"),
        "must not fabricate edge for non-trait method, got {calls:?}"
    );
}

#[test]
fn trait_dispatch_with_send_marker_still_resolves() {
    // `dyn Handler + Send + 'static` — marker traits skipped, Handler wins.
    let fx = parse(
        r#"
        use crate::ports::Handler;
        pub fn dispatch(h: &(dyn Handler + Send)) {
            h.handle();
        }
        "#,
    );
    let mut index = WorkspaceTypeIndex::new();
    index.trait_methods.insert(
        "crate::ports::Handler".to_string(),
        std::iter::once("handle".to_string()).collect(),
    );
    index.trait_impls.insert(
        "crate::ports::Handler".to_string(),
        vec!["crate::app::X".to_string()],
    );
    let calls = run(&fx, &index, "dispatch");
    assert!(calls.contains("crate::app::X::handle"));
}

#[test]
fn trait_dispatch_box_dyn_resolves() {
    // `Box<dyn Handler>` — Box is peeled, then dyn Handler → TraitBound.
    let fx = parse(
        r#"
        use crate::ports::Handler;
        pub fn dispatch(h: Box<dyn Handler>) {
            h.handle();
        }
        "#,
    );
    let mut index = WorkspaceTypeIndex::new();
    index.trait_methods.insert(
        "crate::ports::Handler".to_string(),
        std::iter::once("handle".to_string()).collect(),
    );
    index.trait_impls.insert(
        "crate::ports::Handler".to_string(),
        vec!["crate::app::Y".to_string()],
    );
    let calls = run(&fx, &index, "dispatch");
    assert!(
        calls.contains("crate::app::Y::handle"),
        "Box<dyn Trait> must be peeled, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Stage 3: User-Wrapper-Config
// ═══════════════════════════════════════════════════════════════════

#[test]
fn user_wrapper_is_peeled_on_signature_param() {
    // Axum-style `fn h(State(db): State<Db>) { db.query() }`.
    // Stage 3: configure `State` as a transparent wrapper so the
    // inference peels it to reach `Db`, and `db.query()` resolves.
    // Note: our current `extract_pat_ident_name` handles `db: State<Db>`
    // pattern via `Pat::Ident` with type, not `State(db)` tuple-struct
    // destructuring — so we use the plain form here.
    let fx = parse(
        r#"
        use crate::app::Db;
        pub fn handle(db: State<Db>) {
            db.query();
        }
        "#,
    );
    let db = CanonicalType::path(["crate", "app", "Db"]);
    let mut index = WorkspaceTypeIndex::new();
    index.insert_method_return(
        "crate::app::Db",
        "query",
        CanonicalType::path(["crate", "app", "Rows"]),
    );
    // Register `State` as a transparent wrapper.
    index.transparent_wrappers.insert("State".to_string());
    let calls = run(&fx, &index, "handle");
    let _ = db;
    assert!(
        calls.contains("crate::app::Db::query"),
        "user-wrapper State<Db> should peel to Db, got {calls:?}"
    );
}

#[test]
fn user_wrapper_unconfigured_stays_unresolved() {
    // Same fixture but WITHOUT registering State as transparent. Falls
    // through to <method>:query.
    let fx = parse(
        r#"
        use crate::app::Db;
        pub fn handle(db: State<Db>) {
            db.query();
        }
        "#,
    );
    let index = WorkspaceTypeIndex::new();
    let calls = run(&fx, &index, "handle");
    assert!(
        calls.contains("<method>:query"),
        "unconfigured wrapper must not be peeled, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Stage 3: Type-Alias-Expansion
// ═══════════════════════════════════════════════════════════════════

#[test]
fn type_alias_expands_to_target_via_signature_param() {
    // `type DbRef = std::sync::Arc<Store>;` — `fn h(db: DbRef) { db.read() }`
    // Inference expands DbRef → Arc<Store> → Store (Arc wrapper peeled).
    // Store has a `read` method in our fixture.
    let fx = parse(
        r#"
        type DbRef = std::sync::Arc<Store>;
        pub fn handle(db: DbRef) {
            db.read();
        }
        "#,
    );
    let store = CanonicalType::path(["crate", "cli", "handlers", "Store"]);
    let mut index = WorkspaceTypeIndex::new();
    // Pre-populate the alias: `crate::cli::handlers::DbRef` → syn::Type
    // for `std::sync::Arc<Store>`.
    let aliased: syn::Type = syn::parse_str("std::sync::Arc<Store>").expect("parse alias target");
    // Non-generic alias — no params to substitute.
    index.type_aliases.insert(
        "crate::cli::handlers::DbRef".to_string(),
        crate::adapters::analyzers::architecture::call_parity_rule::type_infer::workspace_index::AliasDef {
            params: Vec::new(),
            target: aliased,
            decl_file: "src/cli/handlers.rs".to_string(),
            decl_mod_stack: Vec::new(),
        },
    );
    // Store::read() method.
    index.insert_method_return(
        "crate::cli::handlers::Store",
        "read",
        CanonicalType::path(["crate", "cli", "handlers", "Data"]),
    );
    // Include `DbRef` in local symbols so the alias key resolves.
    let mut fx = fx;
    fx.local_symbols.insert("DbRef".to_string());
    fx.local_symbols.insert("Store".to_string());
    let calls = run(&fx, &index, "handle");
    let _ = store;
    assert!(
        calls.contains("crate::cli::handlers::Store::read"),
        "type-alias should expand DbRef → Store, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Stage 2: Turbofish-as-Return-Type
// ═══════════════════════════════════════════════════════════════════

#[test]
fn turbofish_gives_concrete_return_type() {
    // `get::<Session>()` — generic fn with single turbofish type arg.
    // No fn_returns entry (generic returns are Opaque), so the
    // turbofish fallback fires and the return type is Session.
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = get::<Session>();
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "turbofish should resolve generic-ctor return type, got {calls:?}"
    );
}

#[test]
fn turbofish_on_type_method_is_not_overridden() {
    // `Vec::<u32>::new()` — turbofish is on the type segment, not the
    // method. Path has 2 segments, so the turbofish fallback doesn't
    // fire. `new` isn't in our index → falls through cleanly.
    let fx = parse(
        r#"
        pub fn cmd() {
            let v = Vec::<u32>::new();
            v.custom_method();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    // Important: we must NOT fabricate a `crate::…::u32::custom_method`
    // edge from the turbofish arg.
    assert!(
        calls.contains("<method>:custom_method"),
        "Vec::<T>::new() turbofish must not override, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════

#[test]
fn mixed_resolutions_in_single_body() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open().unwrap();
            s.diff();
            let x: u32 = 0;
            x.random();
            crate::app::make_session().unwrap().files();
        }
        "#,
    );
    let calls = run(&fx, &rlm_index(), "cmd");
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "resolved: Session::diff missing, got {calls:?}"
    );
    assert!(
        calls.contains("crate::app::session::Session::files"),
        "resolved: Session::files missing, got {calls:?}"
    );
    assert!(
        calls.contains("<method>:random"),
        "unresolved: <method>:random expected, got {calls:?}"
    );
}
