//! `syn::Type` → `CanonicalType` conversion.
//!
//! Recognises `Result` / `Option` / `Future` / `Vec` / `HashMap` /
//! `BTreeMap` and the Deref-transparent smart pointers `Arc` / `Box` /
//! `Rc` / `Cow`, projecting their generic arguments into the matching
//! `CanonicalType` variant. `RwLock` / `Mutex` / `RefCell` / `Cell` are
//! intentionally *not* peeled — their methods (`read`, `lock`,
//! `borrow`, `get`) don't exist on the inner type, and peeling them
//! would synthesize false-positive call-graph edges. Users can opt back
//! in per-wrapper via `[architecture.call_parity]::transparent_wrappers`
//! when a domain-specific wrapper genuinely Derefs to its inner value.
//!
//! Unknown-generic paths resolve through the existing
//! `bindings::canonicalise_type_segments` pipeline (alias map + local
//! symbols + crate roots).
//!
//! Shared between the workspace-index builder and the inference engine
//! — both turn `syn::Type`s into `CanonicalType`s with identical
//! semantics.

use super::super::bindings::{canonicalise_type_segments_in_scope, CanonScope};
use super::super::local_symbols::FileScope;
use super::alias_substitution::substitute_alias_args;
use super::canonical::CanonicalType;
use std::collections::{HashMap, HashSet};

/// Resolution inputs. Per-file lookup tables live in `file`; the
/// remaining fields are workspace-wide or per-call-site.
pub(crate) struct ResolveContext<'a> {
    pub file: &'a FileScope<'a>,
    pub mod_stack: &'a [String],
    /// Workspace-wide type aliases. `None` during pass 1 of the index
    /// build (the alias collector itself); `Some(&…)` afterwards.
    pub type_aliases: Option<&'a HashMap<String, super::workspace_index::AliasDef>>,
    /// User-defined transparent wrappers (`State`, `Extension`, …).
    /// `None` means only stdlib wrappers are peeled.
    pub transparent_wrappers: Option<&'a HashSet<String>>,
    /// Per-file scopes for the whole workspace. `Some(&…)` lets alias
    /// expansion switch to the alias's decl-site scope when resolving
    /// the target — without this, `type Repo = Arc<Store>;` declared
    /// in `domain` and used from `app` would try to resolve `Store` in
    /// `app`'s scope and fail. `None` falls back to using the
    /// use-site's scope (legacy / unit-test path).
    pub workspace_files: Option<&'a HashMap<String, FileScope<'a>>>,
}

/// Hard recursion cap for `resolve_type_with_depth`. Guards against
/// pathological types (`type A = Vec<A>`, deeply nested wrappers, hostile
/// fixtures). Real-world types bottom out well under 16 levels.
const MAX_RESOLVE_DEPTH: u8 = 32;

/// Build a `CanonScope` view over the resolver's context — DRY helper
/// shared by `resolve_bound_list` and `resolve_generic_path`. Operation:
/// pure field projection.
fn canon_scope<'a>(ctx: &'a ResolveContext<'a>) -> CanonScope<'a> {
    CanonScope {
        file: ctx.file,
        mod_stack: ctx.mod_stack,
    }
}

// qual:api
/// Convert a declared / inferred `syn::Type` into a `CanonicalType`.
/// References, parens, and the stdlib-wrapper set are peeled; type paths
/// go through the shared canonicalisation pipeline. Integration.
pub(crate) fn resolve_type(ty: &syn::Type, ctx: &ResolveContext<'_>) -> CanonicalType {
    resolve_type_with_depth(ty, ctx, 0)
}

/// Depth-tracked resolver. Collapses to `Opaque` past
/// `MAX_RESOLVE_DEPTH` so stack overflow can't be triggered by user
/// fixtures (defensive: tests build type aliases and wrapper chains
/// the collector walks unconditionally). Integration: dispatch after a
/// single depth guard — each arm is one-call delegation, own recursion
/// hidden behind closures for IOSP leniency.
// qual:recursive
fn resolve_type_with_depth(ty: &syn::Type, ctx: &ResolveContext<'_>, depth: u8) -> CanonicalType {
    depth_guarded(depth, |next| dispatch_type(ty, ctx, next))
}

/// Run `body` only when the cap isn't exceeded, passing `depth + 1` so
/// callers don't hand-code the increment. Operation.
fn depth_guarded<F>(depth: u8, body: F) -> CanonicalType
where
    F: FnOnce(u8) -> CanonicalType,
{
    if depth >= MAX_RESOLVE_DEPTH {
        return CanonicalType::Opaque;
    }
    body(depth + 1)
}

/// Pure dispatch over the `syn::Type` variants. Every arm delegates
/// (closure-hidden own calls keep this classified as an Operation).
fn dispatch_type(ty: &syn::Type, ctx: &ResolveContext<'_>, next: u8) -> CanonicalType {
    let recurse = |t: &syn::Type| resolve_type_with_depth(t, ctx, next);
    let into_slice = |inner: CanonicalType| CanonicalType::Slice(Box::new(inner));
    match ty {
        syn::Type::Reference(r) => recurse(&r.elem),
        syn::Type::Paren(p) => recurse(&p.elem),
        syn::Type::Path(tp) => resolve_path(&tp.path, ctx, next),
        syn::Type::Array(a) => into_slice(recurse(&a.elem)),
        syn::Type::Slice(s) => into_slice(recurse(&s.elem)),
        syn::Type::TraitObject(tto) => resolve_bound_list(&tto.bounds, ctx),
        // `impl Trait` return type — the concrete type is hidden by the
        // compiler, but we can still extract the first non-marker trait
        // bound and treat the result like `dyn Trait` so trait-dispatch
        // over-approximation fires on the method call.
        syn::Type::ImplTrait(iti) => resolve_bound_list(&iti.bounds, ctx),
        _ => CanonicalType::Opaque,
    }
}

/// Extract the first resolvable non-marker trait bound from a
/// `dyn T1 + T2` or `impl T1 + T2` list and canonicalise it to
/// `TraitBound(path)`. Marker traits (`Send`, `Sync`, `Unpin`, `Copy`,
/// `Clone`, etc.) and lifetime bounds are skipped, as are bounds that
/// can't be canonicalised (external crates not in the workspace) — so
/// `dyn ExternalTrait + LocalTrait` still dispatches via `LocalTrait`.
/// Yields `Opaque` if no resolvable trait bound exists. Operation.
fn resolve_bound_list(
    bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>,
    ctx: &ResolveContext<'_>,
) -> CanonicalType {
    for bound in bounds {
        let syn::TypeParamBound::Trait(trait_bound) = bound else {
            continue;
        };
        if is_marker_trait(&trait_bound.path) {
            continue;
        }
        // `impl Future<Output = T>` deserves the same `Future(T)` shape
        // the path-form `Future<Output = T>` produces, so `.await` on
        // the result resolves through the combinator table.
        if let Some(args) = future_args(&trait_bound.path) {
            return wrap_future_output(args, ctx, 0);
        }
        let segs: Vec<String> = trait_bound
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        if let Some(resolved) = canonicalise_type_segments_in_scope(&segs, &canon_scope(ctx)) {
            return CanonicalType::TraitBound(resolved);
        }
    }
    CanonicalType::Opaque
}

/// Return the path arguments of a `Future` trait bound (covers bare
/// `Future`, `std::future::Future`, and any other path ending in
/// `Future`); `None` when the last segment isn't `Future`.
fn future_args(path: &syn::Path) -> Option<&syn::PathArguments> {
    let last = path.segments.last()?;
    (last.ident == "Future").then_some(&last.arguments)
}

/// Marker traits (plus common auto-derive names) that are skipped when
/// picking the dispatch-relevant trait from a `dyn T1 + T2` bound set.
/// Kept as a const so the list is greppable and easy to extend.
const MARKER_TRAITS: &[&str] = &[
    "Send", "Sync", "Unpin", "Copy", "Clone", "Sized", "Debug", "Display",
];

/// Skip marker traits when picking the dispatch-relevant trait from
/// `dyn T1 + T2`. Operation: lookup table.
fn is_marker_trait(path: &syn::Path) -> bool {
    let Some(last) = path.segments.last() else {
        return false;
    };
    let name = last.ident.to_string();
    MARKER_TRAITS.contains(&name.as_str())
}

/// Dispatch on the last path-segment's ident to recognise stdlib
/// wrappers. Falls through to `resolve_generic_path` for everything
/// else. Integration: closure-hidden own calls keep IOSP clean.
fn resolve_path(path: &syn::Path, ctx: &ResolveContext<'_>, depth: u8) -> CanonicalType {
    let Some(last) = path.segments.last() else {
        return CanonicalType::Opaque;
    };
    let args = &last.arguments;
    let wrap = |idx, ctor: fn(Box<CanonicalType>) -> CanonicalType| {
        wrap_generic(args, idx, ctx, depth, ctor)
    };
    let peel = || peel_single_generic(args, ctx, depth);
    let fallback = || resolve_generic_path(path, ctx, depth);
    let name = last.ident.to_string();
    let wrap_future = || wrap_future_output(args, ctx, depth);
    match name.as_str() {
        "Result" => wrap(0, CanonicalType::Result),
        "Option" => wrap(0, CanonicalType::Option),
        // Future uses `Output = T` associated-type syntax, not a
        // positional generic. Handle both forms in the dedicated helper.
        "Future" => wrap_future(),
        "Vec" => wrap(0, CanonicalType::Slice),
        "HashMap" | "BTreeMap" => wrap(1, CanonicalType::Map),
        // Only peel smart pointers whose `Deref` makes inner methods
        // reachable directly on the wrapper. `RwLock` / `Mutex` /
        // `RefCell` / `Cell` intentionally do NOT deref to their inner
        // value — `db.read()` is `RwLock::read`, not `Inner::read` —
        // so peeling them would synthesize bogus edges to the inner
        // type. Users can opt back in via `transparent_wrappers` for
        // domain-specific deref-like wrappers.
        "Arc" | "Box" | "Rc" | "Cow" => peel(),
        _ if is_user_transparent(&name, ctx) => peel(),
        _ => fallback(),
    }
}

/// Future-specific wrapper: `std::future::Future<Output = T>` uses the
/// `Output = T` associated-type syntax. Accepts the positional form
/// `Future<T>` too as a secondary fallback. Operation.
fn wrap_future_output(
    args: &syn::PathArguments,
    ctx: &ResolveContext<'_>,
    depth: u8,
) -> CanonicalType {
    let recurse = |t: &syn::Type| resolve_type_with_depth(t, ctx, depth);
    match future_output_type(args) {
        Some(inner) => CanonicalType::Future(Box::new(recurse(inner))),
        None => CanonicalType::Opaque,
    }
}

/// Extract the `Output` type from `Future<Output = T>`; fall back to
/// the first positional generic arg for the rarer `Future<T>` form.
/// Operation.
fn future_output_type(args: &syn::PathArguments) -> Option<&syn::Type> {
    let syn::PathArguments::AngleBracketed(ab) = args else {
        return None;
    };
    let assoc = ab.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::AssocType(a) if a.ident == "Output" => Some(&a.ty),
        _ => None,
    });
    assoc.or_else(|| generic_type_arg(args, 0))
}

/// check if `name` is a user-configured transparent wrapper.
/// Operation: set lookup with optional presence.
fn is_user_transparent(name: &str, ctx: &ResolveContext<'_>) -> bool {
    ctx.transparent_wrappers
        .is_some_and(|set| set.contains(name))
}

/// Build a wrapper variant from a recognized generic type at position
/// `idx`. If the argument is absent, returns `Opaque`. Operation:
/// closure-hidden recursion for IOSP leniency.
fn wrap_generic<F>(
    args: &syn::PathArguments,
    idx: usize,
    ctx: &ResolveContext<'_>,
    depth: u8,
    constructor: F,
) -> CanonicalType
where
    F: FnOnce(Box<CanonicalType>) -> CanonicalType,
{
    // `depth` already carries the +1 from `dispatch_type`'s guard —
    // `resolve_type_with_depth` re-applies the guard, so pass through.
    let recurse = |t: &syn::Type| resolve_type_with_depth(t, ctx, depth);
    match generic_type_arg(args, idx) {
        Some(inner) => constructor(Box::new(recurse(inner))),
        None => CanonicalType::Opaque,
    }
}

/// Peel a transparent single-type-param wrapper (Arc / Box / Rc / Cow
/// plus any user-configured `transparent_wrappers`) by recursing into
/// its first generic argument. Operation.
fn peel_single_generic(
    args: &syn::PathArguments,
    ctx: &ResolveContext<'_>,
    depth: u8,
) -> CanonicalType {
    let recurse = |t: &syn::Type| resolve_type_with_depth(t, ctx, depth);
    match generic_type_arg(args, 0) {
        Some(inner) => recurse(inner),
        None => CanonicalType::Opaque,
    }
}

/// Resolve a non-wrapper path through the shared canonicalisation
/// pipeline. On an alias hit, substitute use-site generic args into
/// the alias target and recursively resolve it against the alias's
/// *own* declaring scope — without that, an imported alias like
/// `type Repo = Arc<Store>` would try to resolve `Store` in the
/// use-site's scope, where it isn't necessarily known.
fn resolve_generic_path(path: &syn::Path, ctx: &ResolveContext<'_>, depth: u8) -> CanonicalType {
    let canonicalise =
        |segs: &[String]| canonicalise_type_segments_in_scope(segs, &canon_scope(ctx));
    let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let Some(resolved) = canonicalise(&segments) else {
        return CanonicalType::Opaque;
    };
    let key = resolved.join("::");
    if let Some(alias) = ctx.type_aliases.and_then(|m| m.get(&key)) {
        let expanded = substitute_alias_args(&alias.target, &alias.params, path);
        return resolve_in_alias_scope(&expanded, alias, ctx, depth);
    }
    CanonicalType::Path(resolved)
}

/// Resolve `target` (an alias body, post-substitution) against the
/// alias's declaring scope. Falls back to the use-site scope when
/// `workspace_files` lacks an entry for `decl_file` (legacy / unit-
/// test paths). Operation: scope swap + recurse.
fn resolve_in_alias_scope(
    target: &syn::Type,
    alias: &super::workspace_index::AliasDef,
    ctx: &ResolveContext<'_>,
    depth: u8,
) -> CanonicalType {
    let decl_file = ctx
        .workspace_files
        .and_then(|files| files.get(&alias.decl_file));
    let Some(decl_file) = decl_file else {
        return resolve_type_with_depth(target, ctx, depth);
    };
    let decl_ctx = ResolveContext {
        file: decl_file,
        mod_stack: &alias.decl_mod_stack,
        type_aliases: ctx.type_aliases,
        transparent_wrappers: ctx.transparent_wrappers,
        workspace_files: ctx.workspace_files,
    };
    resolve_type_with_depth(target, &decl_ctx, depth)
}

/// Extract the type at position `idx` from angle-bracketed generic args.
/// Lifetimes / const args are skipped; only type args count.
fn generic_type_arg(args: &syn::PathArguments, idx: usize) -> Option<&syn::Type> {
    let syn::PathArguments::AngleBracketed(ab) = args else {
        return None;
    };
    ab.args
        .iter()
        .filter_map(|a| match a {
            syn::GenericArgument::Type(t) => Some(t),
            _ => None,
        })
        .nth(idx)
}
