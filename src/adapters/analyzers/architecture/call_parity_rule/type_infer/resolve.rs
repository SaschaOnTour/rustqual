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
use super::canonical::CanonicalType;
use super::resolve_alias::{expand_alias, lookup_alias_param};
use super::resolve_marker::is_marker_trait;
use super::resolve_wrapper::identify_wrapper_name;
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
    /// Active inside an alias body: param-name → canonical type
    /// pre-resolved in the *use-site* scope. Without this, the body
    /// resolves naked `T` against the alias's decl-site, which can't
    /// see the use-site's symbols. `None` outside alias expansion.
    pub alias_param_subs: Option<&'a HashMap<String, CanonicalType>>,
    /// Fn-scoped generic params with their trait bounds — populated
    /// by `extract_method_generic_params` and threaded through the
    /// collector so `resolve_path` can return `TraitBound(...)` for
    /// a bare-ident path that names a generic type parameter
    /// (`Q` in `fn f<Q: Trait>(q: &Q) { q.method() }`). `None` for
    /// workspace-pass-1 / alias-body / test contexts that have no
    /// fn-level generic info.
    pub generic_params: Option<&'a HashMap<String, Vec<Vec<String>>>>,
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
pub(super) fn resolve_type_with_depth(
    ty: &syn::Type,
    ctx: &ResolveContext<'_>,
    depth: u8,
) -> CanonicalType {
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
    let path = |tp: &syn::TypePath| {
        lookup_alias_param(tp, ctx).unwrap_or_else(|| resolve_path(&tp.path, ctx, next))
    };
    match ty {
        syn::Type::Reference(r) => recurse(&r.elem),
        syn::Type::Paren(p) => recurse(&p.elem),
        syn::Type::Path(tp) => path(tp),
        syn::Type::Array(a) => into_slice(recurse(&a.elem)),
        syn::Type::Slice(s) => into_slice(recurse(&s.elem)),
        syn::Type::TraitObject(tto) => resolve_bound_list(&tto.bounds, ctx, next),
        // `impl Trait` return type — the concrete type is hidden by
        // the compiler, but we collect every non-marker trait bound
        // and treat the result like `dyn Trait` so trait-dispatch
        // over-approximation fires per bound on the method call.
        syn::Type::ImplTrait(iti) => resolve_bound_list(&iti.bounds, ctx, next),
        _ => CanonicalType::Opaque,
    }
}

/// Collect every resolvable non-marker trait bound from a
/// `dyn T1 + T2` or `impl T1 + T2` list and canonicalise to
/// `TraitBound(Vec<path>)`. Marker traits (`Send`, `Sync`, `Unpin`,
/// `Copy`, `Clone`, etc.) and lifetime bounds are skipped; bounds
/// that can't be canonicalised (external crates not in the
/// workspace) are filtered out so `dyn ExternalTrait + LocalTrait`
/// still dispatches via `LocalTrait`. A `Future<Output = T>` bound
/// short-circuits to `Future(T)` (combinator-table compatibility) —
/// in that case any peer trait bounds are skipped because
/// `CanonicalType` can't represent both `Future` and `TraitBound`
/// simultaneously. Yields `Opaque` if no resolvable trait bound
/// exists. Operation.
fn resolve_bound_list(
    bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>,
    ctx: &ResolveContext<'_>,
    depth: u8,
) -> CanonicalType {
    let mut collected: Vec<Vec<String>> = Vec::new();
    for bound in bounds {
        let syn::TypeParamBound::Trait(trait_bound) = bound else {
            continue;
        };
        if is_marker_trait(&trait_bound.path, ctx) {
            continue;
        }
        // `impl Future<Output = T>` deserves the same `Future(T)` shape
        // the path-form produces, so `.await` resolves through the
        // combinator table. Future short-circuits even when peer
        // trait bounds exist — `CanonicalType` can't carry both.
        if let Some(args) = future_bound_args(trait_bound, ctx) {
            return wrap_future_output(args, ctx, depth);
        }
        let segs: Vec<String> = trait_bound
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        // Trait dispatch only knows the workspace, so only crate-rooted
        // canonicals count. External aliases like `serde::Serialize`
        // resolve to `["serde", "Serialize"]` and are filtered out.
        if let Some(resolved) = canonicalise_type_segments_in_scope(&segs, &canon_scope(ctx)) {
            if resolved.first().map(String::as_str) == Some("crate") {
                collected.push(resolved);
            }
        }
    }
    if collected.is_empty() {
        CanonicalType::Opaque
    } else {
        CanonicalType::TraitBound(collected)
    }
}

/// Return the path arguments of a `Future` trait bound when the bound
/// canonically resolves to `std::future::Future` — covers bare
/// `Future`, fully-qualified `std::future::Future`, and aliased forms
/// like `use std::future::Future as Fut;` followed by `impl Fut<Output = T>`.
/// `None` when the bound isn't a Future variant. The returned
/// `PathArguments` come from the original (aliased) leaf so the
/// `Output = T` associated type stays accessible.
fn future_bound_args<'a>(
    trait_bound: &'a syn::TraitBound,
    ctx: &ResolveContext<'_>,
) -> Option<&'a syn::PathArguments> {
    let last = trait_bound.path.segments.last()?;
    let raw_name = last.ident.to_string();
    let wrapper = identify_wrapper_name(&trait_bound.path, &raw_name, ctx)?;
    (wrapper == "Future").then_some(&last.arguments)
}

/// Names of the recognised stdlib wrappers, used both for direct-name
/// dispatch in `resolve_path` and for the alias-aware lookup that
/// promotes `use std::sync::Arc as Shared;`-imported names to their
/// canonical wrapper.
pub(super) const WRAPPER_NAMES: &[&str] = &[
    "Result", "Option", "Future", "Vec", "HashMap", "BTreeMap", "Arc", "Box", "Rc", "Cow",
];

/// True when the canonical path starts with `std`, `core`, or
/// `alloc` — the prefixes that distinguish a real stdlib import
/// (`use std::sync::Arc as Shared;`) from a local one with a
/// matching leaf (`use crate::wrap::Arc as Shared;`). Shared by the
/// receiver-type resolver (this module) and the visibility pass
/// (`pub_fns_visibility`) so both apply the same alias-promotion
/// rules. Operation.
pub(crate) fn is_stdlib_prefixed(canonical: &[String]) -> bool {
    matches!(
        canonical.first().map(String::as_str),
        Some("std" | "core" | "alloc")
    )
}

/// Dispatch on the last path-segment's ident to recognise stdlib
/// wrappers. Resolves `use std::sync::Arc as Shared;`-style import
/// aliases first, so `Shared<T>` peels just like `Arc<T>`. Falls
/// through to `resolve_generic_path` for everything else.
/// Integration: closure-hidden own calls keep IOSP clean.
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
    let wrap_future = || wrap_future_output(args, ctx, depth);
    let raw_name = last.ident.to_string();
    // `identify_wrapper_name` is authoritative: it considers the
    // canonical resolution (catching shadow cases like
    // `use crate::wrap::Arc;`), explicit-stdlib qualification, and
    // user-transparent leaf matches. When it returns `None`, the
    // path isn't a wrapper — drop straight into the regular
    // canonicalisation pipeline.
    let Some(name) = identify_wrapper_name(path, &raw_name, ctx) else {
        return fallback();
    };
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

/// Check if `name` is a user-configured transparent wrapper.
/// Operation: set lookup with optional presence.
pub(super) fn is_user_transparent(name: &str, ctx: &ResolveContext<'_>) -> bool {
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
/// pipeline. On an alias hit, delegate to `resolve_alias::expand_alias`
/// to handle param substitution + decl-site scope swap. Single-segment
/// paths that name a fn-scoped generic param (`Q` where `Q: T1 + T2`)
/// short-circuit to `TraitBound(all-bounds)` so receiver-position
/// dispatch can fan out one edge per bound — without this, the
/// canonicalisation pipeline would either find no workspace symbol
/// (→ `Opaque`) or accidentally collide with an unrelated workspace
/// type (→ wrong `Path`).
fn resolve_generic_path(path: &syn::Path, ctx: &ResolveContext<'_>, depth: u8) -> CanonicalType {
    let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if let Some(bounds) = generic_param_bounds(&segments, ctx) {
        return CanonicalType::TraitBound(bounds);
    }
    let canonicalise =
        |segs: &[String]| canonicalise_type_segments_in_scope(segs, &canon_scope(ctx));
    let Some(resolved) = canonicalise(&segments) else {
        return CanonicalType::Opaque;
    };
    let key = resolved.join("::");
    if let Some(alias) = ctx.type_aliases.and_then(|m| m.get(&key)) {
        return expand_alias(alias, path, ctx, depth);
    }
    CanonicalType::Path(resolved)
}

/// Single-segment path matching a known generic param → return all
/// of its non-empty trait bounds (canonical segments). Multi-bound
/// (`Q: T1 + T2`) yields all bounds so `canonical_edges_for_method`
/// can fan out one edge per bound — same behaviour as the UFCS
/// branch in `canonicalise_generic_param_path`. Returns `None` if no
/// fn-level `generic_params` available, the path is multi-segment,
/// or the matched param has only empty bounds (unbounded `<Q>`).
fn generic_param_bounds(segments: &[String], ctx: &ResolveContext<'_>) -> Option<Vec<Vec<String>>> {
    if segments.len() != 1 {
        return None;
    }
    let bounds = ctx.generic_params?.get(&segments[0])?;
    let collected: Vec<Vec<String>> = bounds.iter().filter(|b| !b.is_empty()).cloned().collect();
    if collected.is_empty() {
        None
    } else {
        Some(collected)
    }
}

/// Extract the type at position `idx` from angle-bracketed generic args.
/// Lifetimes / const args are skipped; only type args count.
pub(super) fn generic_type_arg(args: &syn::PathArguments, idx: usize) -> Option<&syn::Type> {
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
