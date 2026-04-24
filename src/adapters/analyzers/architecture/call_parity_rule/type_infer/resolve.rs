//! `syn::Type` → `CanonicalType` conversion.
//!
//! Recognises stdlib wrappers (`Result`, `Option`, `Vec`, `HashMap`,
//! `BTreeMap`, `Arc`, `Box`, `Rc`, `Cow`, `RwLock`, `Mutex`, `RefCell`,
//! `Cell`) and projects their generic arguments into the matching
//! `CanonicalType` variant. Unknown-generic paths resolve through the
//! existing `bindings::canonicalise_type_segments` pipeline (alias map
//! + local symbols + crate roots).
//!
//! Shared between the workspace-index builder (Task 1.2) and the
//! inference engine (Task 1.3) — both turn `syn::Type`s into
//! `CanonicalType`s with identical semantics.

use super::super::bindings::canonicalise_type_segments;
use super::alias_substitution::substitute_alias_args;
use super::canonical::CanonicalType;
use std::collections::{HashMap, HashSet};

/// Resolution inputs, bundled so the recursive calls don't drag a long
/// parameter list around.
pub(crate) struct ResolveContext<'a> {
    pub alias_map: &'a HashMap<String, Vec<String>>,
    pub local_symbols: &'a HashSet<String>,
    pub crate_root_modules: &'a HashSet<String>,
    pub importing_file: &'a str,
    /// Stage 3 workspace-wide type aliases. `None` means the caller
    /// doesn't need alias expansion (the workspace-index build phase,
    /// where the alias map is still being populated). Inference paths
    /// pass `Some(&workspace.type_aliases)`. The stored tuple carries
    /// the alias's generic-param names plus its target — use-site args
    /// are substituted into the target before recursion.
    pub type_aliases: Option<&'a HashMap<String, (Vec<String>, syn::Type)>>,
    /// Stage 3 user-defined transparent wrappers — the last-ident
    /// names (e.g. `"State"`, `"Extension"`, `"Data"`) that are peeled
    /// just like `Arc` / `Box`. `None` means only stdlib wrappers are
    /// peeled.
    pub transparent_wrappers: Option<&'a HashSet<String>>,
}

/// Hard recursion cap for `resolve_type_with_depth`. Guards against
/// pathological types (`type A = Vec<A>`, deeply nested wrappers, hostile
/// fixtures). Real-world types bottom out well under 16 levels.
const MAX_RESOLVE_DEPTH: u8 = 32;

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
        let segs: Vec<String> = trait_bound
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        if let Some(resolved) = canonicalise_type_segments(
            &segs,
            ctx.alias_map,
            ctx.local_symbols,
            ctx.crate_root_modules,
            ctx.importing_file,
        ) {
            return CanonicalType::TraitBound(resolved);
        }
    }
    CanonicalType::Opaque
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

/// Stage 3 — check if `name` is a user-configured transparent wrapper.
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

/// Peel a transparent single-type-param wrapper (Arc / Box / Rc / Cow /
/// RwLock / Mutex / RefCell / Cell) by recursing into its first generic
/// argument. Operation.
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
/// pipeline (alias map / local symbols / crate roots). If the canonical
/// matches a recorded workspace type-alias, the alias target is
/// substituted with use-site generic args and recursively resolved.
/// Operation: closure-hidden calls + alias dispatch.
fn resolve_generic_path(path: &syn::Path, ctx: &ResolveContext<'_>, depth: u8) -> CanonicalType {
    let recurse = |t: &syn::Type| resolve_type_with_depth(t, ctx, depth);
    let canonicalise = |segs: &[String]| {
        canonicalise_type_segments(
            segs,
            ctx.alias_map,
            ctx.local_symbols,
            ctx.crate_root_modules,
            ctx.importing_file,
        )
    };
    let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let Some(resolved) = canonicalise(&segments) else {
        return CanonicalType::Opaque;
    };
    let key = resolved.join("::");
    if let Some((params, target)) = ctx.type_aliases.and_then(|m| m.get(&key)) {
        let expanded = substitute_alias_args(target, params, path);
        return recurse(&expanded);
    }
    CanonicalType::Path(resolved)
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
