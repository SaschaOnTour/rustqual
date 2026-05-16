//! Fn-signature parameter extraction.
//!
//! Shared by `pub_fns` (Check B pub-fn collection) and `workspace_graph`
//! (graph-build) — both need the same `(name, &Type)` pairs that the
//! `CanonicalCallCollector` seeds into its binding scope.

use super::bindings::{canonicalise_type_segments_in_scope, CanonScope};
use super::local_symbols::FileScope;
use std::collections::HashMap;

// qual:api
/// Per-param entry in the canonical generics map: the canonicalised
/// trait bounds for the param plus its turbofish substitution position
/// (`Some(idx)` for params reachable via a call-site turbofish — fn-own
/// params for free fns, method-own params for methods, struct-own
/// params for fields. `None` for impl-level params merged into a method
/// context: those are determined by receiver-type inference, not by
/// the method-call turbofish).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamInfo {
    pub bounds: Vec<Vec<String>>,
    pub turbofish_index: Option<usize>,
}

// qual:api
/// Single source of truth for "does this path qualify as an in-scope
/// generic-param shadow against `generic_params`?" The gate enforces
/// two invariants every caller must respect:
/// - Absolute paths (`::Q`, `path.leading_colon.is_some()`) are the
///   caller's explicit disambiguation AWAY from in-scope generics →
///   never match. Pre-extraction, this check landed in the type
///   resolver but was missed in the call collector, so `::Q::handle()`
///   inside `fn run<Q: Handler>(...)` still mis-routed through the
///   `Handler::handle` anchor.
/// - The head segment must be a known generic param name.
///
/// Callers decide what to do with the match — the type resolver
/// short-circuits to `GenericParamBound` for single-segment paths,
/// the call collector fans out one anchor edge per bound for
/// multi-segment `Q::method` dispatches — but they ALL go through
/// this helper to look up the param. Direct `generic_params.get(name)`
/// in any new code is a drift trap; route through here instead.
/// Operation.
pub(crate) fn matched_generic_param<'a>(
    segments: &[String],
    leading_colon_set: bool,
    generic_params: &'a HashMap<String, ParamInfo>,
) -> Option<&'a ParamInfo> {
    if leading_colon_set {
        return None;
    }
    generic_params.get(segments.first()?)
}

/// Extract `(name, &Type)` pairs for every typed positional parameter
/// of a fn signature. Framework-extractor patterns like
/// `fn h(State(db): State<Db>)` contribute `("db", State<Db>)` — the
/// outer type still goes through `resolve_type`, which peels the
/// transparent wrapper to reach `Db` when `State` is configured in
/// `transparent_wrappers`.
pub(crate) fn extract_signature_params(sig: &syn::Signature) -> Vec<(String, &syn::Type)> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pt) => {
                param_name_from_pat(pt.pat.as_ref()).map(|n| (n, pt.ty.as_ref()))
            }
            _ => None,
        })
        .collect()
}

// qual:api
/// Canonical generic-param map for a free fn / struct / impl-block —
/// any item whose generics aren't merged with an outer scope. Returns
/// `{name → ParamInfo}` with each param's `turbofish_index` set to its
/// position in the item's own generics list (all params are call-site
/// substitutable in this shape). Entries with empty bound lists are
/// kept so `generic_param_shadow` still recognises the param as a
/// generic (shadows same-named workspace symbols). Integration: one
/// helper owns the whole pipeline so call sites can't compose the
/// atomic steps incorrectly.
pub(crate) fn item_canonical_generics(
    generics: &syn::Generics,
    file: &FileScope<'_>,
    mod_stack: &[String],
) -> HashMap<String, ParamInfo> {
    let raw = extract_item_generics(generics);
    let mut out = HashMap::new();
    for (idx, (name, bounds)) in raw.into_iter().enumerate() {
        out.insert(
            name,
            ParamInfo {
                bounds: canonicalise_bounds(&bounds, file, mod_stack),
                turbofish_index: Some(idx),
            },
        );
    }
    out
}

// qual:api
/// Canonical generic-param map for a method inside an `impl` block.
/// Tags each param with its origin so turbofish substitution at the
/// call site only fires for method-own params:
/// - Method-own params (`fn current<Q: T>(&self) -> Q`) get
///   `turbofish_index = Some(position-in-method-sig)`. The call-site
///   turbofish `s.current::<X>()` substitutes them.
/// - Impl-level params (`impl<I> S<I> { ... }`) get
///   `turbofish_index = None` — they're determined by the receiver
///   type, not by a method-call turbofish.
///
/// Method-level `where Q: T` predicates that target an impl-level
/// generic merge onto the impl-level entry (still `None`-indexed) so
/// the bound is captured but no turbofish substitution is attempted.
/// Integration: the only correct way to assemble the generic-params
/// map for a method.
pub(crate) fn method_canonical_generics(
    sig: &syn::Signature,
    impl_generics: &[(String, Vec<Vec<String>>)],
    file: &FileScope<'_>,
    mod_stack: &[String],
) -> HashMap<String, ParamInfo> {
    let outer_names: Vec<&str> = impl_generics.iter().map(|(n, _)| n.as_str()).collect();
    let method_raw = extract_method_with_outer(sig, &outer_names);
    let method_positions: HashMap<String, usize> = sig
        .generics
        .params
        .iter()
        .filter_map(|p| match p {
            syn::GenericParam::Type(tp) => Some(tp.ident.to_string()),
            _ => None,
        })
        .enumerate()
        .map(|(i, n)| (n, i))
        .collect();
    let mut out = HashMap::new();
    // Impl-level entries: turbofish_index = None (receiver-driven).
    for (name, bounds) in impl_generics {
        out.insert(
            name.clone(),
            ParamInfo {
                bounds: canonicalise_bounds(bounds, file, mod_stack),
                turbofish_index: None,
            },
        );
    }
    // Method-level + outer-extending entries: merge bounds onto existing
    // impl-level entries (preserving their None index), or insert fresh
    // entries with the method's own position.
    for (name, bounds) in method_raw {
        let canonical = canonicalise_bounds(&bounds, file, mod_stack);
        match out.get_mut(&name) {
            Some(existing) => existing.bounds.extend(canonical),
            None => {
                let turbofish_index = method_positions.get(&name).copied();
                out.insert(
                    name,
                    ParamInfo {
                        bounds: canonical,
                        turbofish_index,
                    },
                );
            }
        }
    }
    out
}

// qual:api
/// Raw impl-level generic-params for use as `outer_names` /
/// `impl_generics` input to `method_canonical_generics`. Visitor
/// frames hold this so children can refer to the impl's params.
/// `extract_*` variants are private; this exposes only the visitor-
/// stack form that callers actually need.
pub(crate) fn impl_block_generics(generics: &syn::Generics) -> Vec<(String, Vec<Vec<String>>)> {
    extract_item_generics(generics)
}

/// Method-only extractor: inline bounds + where-bounds (extending if
/// the bound targets an `outer_names` entry, i.e. an impl-level
/// generic referenced from the method's where clause). Without this
/// extending pass, `impl<Q> R<Q> { fn f(&self) where Q: Handler }`
/// would lose `Handler`. Private: only `method_canonical_generics`
/// composes this with the canonicaliser.
fn extract_method_with_outer(
    sig: &syn::Signature,
    outer_names: &[&str],
) -> Vec<(String, Vec<Vec<String>>)> {
    let mut bounds_by_name = collect_inline_bounds(&sig.generics);
    if let Some(where_clause) = sig.generics.where_clause.as_ref() {
        merge_where_bounds_extending(&mut bounds_by_name, where_clause, outer_names);
    }
    bounds_by_name
}

/// Item-level extractor: inline bounds + where-bounds (only for names
/// already declared as inline params — no extending). Used by free
/// fns, structs, and impl blocks. Private: composed by
/// `item_canonical_generics` and `impl_block_generics`.
fn extract_item_generics(generics: &syn::Generics) -> Vec<(String, Vec<Vec<String>>)> {
    let mut bounds_by_name = collect_inline_bounds(generics);
    if let Some(where_clause) = generics.where_clause.as_ref() {
        merge_where_bounds(&mut bounds_by_name, where_clause);
    }
    bounds_by_name
}

/// Project `Generics::params` to `(name, inline_bounds)` for every
/// type param. Lifetime / const params are skipped.
fn collect_inline_bounds(generics: &syn::Generics) -> Vec<(String, Vec<Vec<String>>)> {
    generics
        .params
        .iter()
        .filter_map(|p| match p {
            syn::GenericParam::Type(tp) => {
                Some((tp.ident.to_string(), trait_bound_paths(&tp.bounds)))
            }
            _ => None,
        })
        .collect()
}

/// For each `T: Trait` predicate where `T` is a single-ident type,
/// append the trait bounds to the matching entry in `bounds_by_name`.
/// Predicates with non-trivial bounded types (`Vec<T>: ...`,
/// `T::Assoc: ...`) are dropped — they don't affect bare `T::method()`
/// dispatch resolution.
fn merge_where_bounds(
    bounds_by_name: &mut [(String, Vec<Vec<String>>)],
    where_clause: &syn::WhereClause,
) {
    for_each_simple_predicate(where_clause, |name, bounds| {
        if let Some(entry) = bounds_by_name.iter_mut().find(|(n, _)| n == &name) {
            entry.1.extend(bounds);
        }
    });
}

/// Like `merge_where_bounds`, but if the predicate's name isn't in
/// `bounds_by_name` AND is in `extending_names`, push a fresh entry.
/// Used by the method-side extractor so a method's `where Q: Trait`
/// for an impl-level `Q` produces an entry that survives the
/// downstream merge in `method_canonical_generics` (which accumulates
/// the where-clause bound onto the impl-level entry there).
fn merge_where_bounds_extending(
    bounds_by_name: &mut Vec<(String, Vec<Vec<String>>)>,
    where_clause: &syn::WhereClause,
    extending_names: &[&str],
) {
    for_each_simple_predicate(where_clause, |name, bounds| {
        if let Some(entry) = bounds_by_name.iter_mut().find(|(n, _)| n == &name) {
            entry.1.extend(bounds);
        } else if extending_names.iter().any(|n| *n == name) {
            bounds_by_name.push((name, bounds));
        }
    });
}

fn for_each_simple_predicate<F: FnMut(String, Vec<Vec<String>>)>(
    where_clause: &syn::WhereClause,
    mut sink: F,
) {
    for predicate in &where_clause.predicates {
        let syn::WherePredicate::Type(pt) = predicate else {
            continue;
        };
        let Some(name) = single_ident_type(&pt.bounded_ty) else {
            continue;
        };
        sink(name, trait_bound_paths(&pt.bounds));
    }
}

/// `T` (single-segment, no generics) → Some("T"); anything else None.
fn single_ident_type(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(p) = ty else {
        return None;
    };
    super::type_infer::single_ident_of(p)
}

/// Flatten each trait bound into its segment idents. Lifetime bounds
/// and `?Sized`-style negative bounds are dropped. Extern-root bounds
/// (`Q: ::ext::Trait`, Rust 2018+) are also dropped here: the leading
/// colon explicitly disambiguates AWAY from workspace symbols, so the
/// bound contributes no dispatchable anchor to the call graph, and
/// keeping the bare segments would let the downstream
/// `canonicalise_bounds` mis-resolve them to a same-named workspace
/// trait. Operation: per-bound projection + leading-colon filter.
fn trait_bound_paths(
    bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>,
) -> Vec<Vec<String>> {
    bounds
        .iter()
        .filter_map(|b| match b {
            syn::TypeParamBound::Trait(tb) => {
                if tb.path.leading_colon.is_some() {
                    return None;
                }
                Some(
                    tb.path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect(),
                )
            }
            _ => None,
        })
        .collect()
}

/// Pull the bound identifier out of a fn-parameter pattern. Supports
/// `Pat::Ident` (the 99% case) and single-ident `Pat::TupleStruct`
/// destructuring (framework extractors: `State(db)`, `Extension(ext)`,
/// `Path(p)`, `Json(body)`, `Data(ctx)`). Returns `None` for deeper
/// destructuring that the resolver can't express yet.
/// Operation: pattern peel.
fn param_name_from_pat(pat: &syn::Pat) -> Option<String> {
    match pat {
        syn::Pat::Ident(pi) => Some(pi.ident.to_string()),
        syn::Pat::TupleStruct(ts) if ts.elems.len() == 1 => {
            if let syn::Pat::Ident(pi) = &ts.elems[0] {
                return Some(pi.ident.to_string());
            }
            None
        }
        _ => None,
    }
}

/// Canonicalise one raw trait-bound segment list against `file`'s
/// scope. Bounds that resolve become anchor prefixes; unresolvable
/// bounds (external trait, typo) are dropped from the result. The
/// param entry that carries this list stays in the canonical map
/// regardless of bound resolvability (so `generic_param_shadow` still
/// recognises the param as a generic), just with an empty bound list
/// that produces zero anchor edges. Private: composed by
/// `item_canonical_generics` / `method_canonical_generics`.
fn canonicalise_bounds(
    bounds: &[Vec<String>],
    file: &FileScope<'_>,
    mod_stack: &[String],
) -> Vec<Vec<String>> {
    let scope = CanonScope { file, mod_stack };
    bounds
        .iter()
        .filter_map(|b| canonicalise_type_segments_in_scope(b, &scope))
        .filter(|c| c.first().map(String::as_str) == Some("crate"))
        .collect()
}
