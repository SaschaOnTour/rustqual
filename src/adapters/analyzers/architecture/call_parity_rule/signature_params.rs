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
