//! Fn-signature parameter extraction.
//!
//! Shared by `pub_fns` (Check B pub-fn collection) and `workspace_graph`
//! (graph-build) — both need the same `(name, &Type)` pairs that the
//! `CanonicalCallCollector` seeds into its binding scope.

// qual:api
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
/// Extract `(name, [trait_bound_path_segs, ...])` for every type
/// parameter declared in a fn signature. Lifetime / const generics
/// are skipped. Each trait bound is returned as its raw path segments
/// (un-canonicalised); the call collector resolves them against the
/// file scope before storing — same pattern `extract_signature_params`
/// uses for value-param types.
///
/// Used so a body call like `Q::execute(&q)` (where `Q: SymbolQuery`)
/// can route through the workspace's trait-method anchor instead of
/// collapsing to `<bare>:Q::execute`.
pub(crate) fn extract_generic_params(sig: &syn::Signature) -> Vec<(String, Vec<Vec<String>>)> {
    sig.generics
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
