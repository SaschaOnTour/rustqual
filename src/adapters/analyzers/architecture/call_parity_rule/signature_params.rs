//! Fn-signature parameter extraction.
//!
//! Shared by `pub_fns` (Check B pub-fn collection) and `workspace_graph`
//! (graph-build) — both need the same `(name, &Type)` pairs that the
//! `CanonicalCallCollector` seeds into its binding scope.

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

/// Extract `(name, [trait_bound_path_segs, ...])` for every type
/// parameter declared in a fn signature, plus any method-level
/// `where`-clause bounds that target an `outer_names` entry — i.e.
/// an impl-level generic referenced from the method's where clause:
/// `impl<Q> Runner<Q> { fn run(&self) where Q: Handler { Q::handle() } }`.
/// Without the outer-name pass, that `where Q: Handler` would be
/// silently dropped (Q is not in the method's own `params`), so
/// `Q::handle()` would fail to canonicalise to `Handler::handle`.
/// Lifetime / const generics are skipped; each trait bound is
/// returned as its raw path segments — the call collector resolves
/// them against the file scope.
pub(crate) fn extract_method_generic_params(
    sig: &syn::Signature,
    outer_names: &[&str],
) -> Vec<(String, Vec<Vec<String>>)> {
    let mut bounds_by_name = collect_inline_bounds(&sig.generics);
    if let Some(where_clause) = sig.generics.where_clause.as_ref() {
        merge_where_bounds_extending(&mut bounds_by_name, where_clause, outer_names);
    }
    bounds_by_name
}

/// Same shape as `extract_method_generic_params` but reads from any
/// `syn::Generics` — used to capture impl-level bounds
/// (`impl<Q: Trait> Foo<Q> { ... }`) which apply to every method's
/// body. Callers merge impl-level + method-level via
/// `merge_generic_params` so a `Q::method()` call inside the body
/// resolves regardless of whether `Q: Trait` is on the impl or the
/// method signature.
pub(crate) fn extract_generics(generics: &syn::Generics) -> Vec<(String, Vec<Vec<String>>)> {
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

/// Combine impl-level and method-level generic params. Same-name
/// generics across impl and method are forbidden by Rust (E0403:
/// "the name `Q` is already used for a generic parameter in this
/// item's generic parameters"), so the only way a name appears in
/// both lists is when method-level `where`-clauses ADD bounds to an
/// impl-level param via `extract_method_generic_params` (which
/// extends `bounds_by_name` with an outer-name entry when the
/// method's where clause references an impl-level generic).
/// Concatenation is therefore correct: it accumulates the full
/// bound set for a param that was introduced once at the impl level.
pub(crate) fn merge_generic_params(
    outer: Vec<(String, Vec<Vec<String>>)>,
    inner: Vec<(String, Vec<Vec<String>>)>,
) -> Vec<(String, Vec<Vec<String>>)> {
    let mut out = outer;
    for (name, bounds) in inner {
        match out.iter_mut().find(|(n, _)| n == &name) {
            Some(entry) => entry.1.extend(bounds),
            None => out.push((name, bounds)),
        }
    }
    out
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
/// for an impl-level `Q` produces an entry that survives into the
/// `merge_generic_params` step (where it accumulates onto the
/// impl-level entry).
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
/// and `?Sized`-style negative bounds are dropped. Operation:
/// per-bound projection.
fn trait_bound_paths(
    bounds: &syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]>,
) -> Vec<Vec<String>> {
    bounds
        .iter()
        .filter_map(|b| match b {
            syn::TypeParamBound::Trait(tb) => Some(
                tb.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect(),
            ),
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
