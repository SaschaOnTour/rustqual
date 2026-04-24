//! Generic-alias argument substitution.
//!
//! Turns `Alias<ArgA, ArgB>` at a use site plus a stored alias
//! definition `type Alias<P1, P2> = Target` into the concrete `Target`
//! with `P1 → ArgA` and `P2 → ArgB` applied. Called from
//! `resolve::resolve_generic_path` when a path canonicalises to a
//! recorded type alias — without this, generic aliases like
//! `type AppResult<T> = Result<T, Error>` would cache `Result<T,
//! Error>` with `T` unbound and `.unwrap()` would yield `Opaque`.

use std::collections::HashMap;
use syn::visit_mut::VisitMut;

// qual:api
/// Substitute an alias's generic parameters with the use-site's type
/// arguments, cloning the stored target and rewriting each `Type::Path`
/// whose single ident matches a param name. Falls back to an
/// unmodified clone when the counts don't line up — the target's
/// remaining unbound params will resolve to `Opaque` downstream,
/// matching the pre-Stage-3 behaviour. Operation.
pub(super) fn substitute_alias_args(
    target: &syn::Type,
    params: &[String],
    use_site: &syn::Path,
) -> syn::Type {
    let mut expanded = target.clone();
    if params.is_empty() {
        return expanded;
    }
    let args = use_site_type_args(use_site);
    if args.len() != params.len() {
        return expanded;
    }
    let subs: HashMap<&str, &syn::Type> = params
        .iter()
        .map(String::as_str)
        .zip(args)
        .collect();
    AliasSubstitutor { subs: &subs }.visit_type_mut(&mut expanded);
    expanded
}

/// Extract the use-site type arguments from the last segment of a
/// path. Lifetime/const args are skipped; only `Type` args count.
/// Operation.
fn use_site_type_args(path: &syn::Path) -> Vec<&syn::Type> {
    let Some(last) = path.segments.last() else {
        return Vec::new();
    };
    let syn::PathArguments::AngleBracketed(ab) = &last.arguments else {
        return Vec::new();
    };
    ab.args
        .iter()
        .filter_map(|a| match a {
            syn::GenericArgument::Type(t) => Some(t),
            _ => None,
        })
        .collect()
}

/// `VisitMut` adapter that replaces single-segment type idents matching
/// an alias param with the corresponding use-site type.
struct AliasSubstitutor<'a> {
    subs: &'a HashMap<&'a str, &'a syn::Type>,
}

impl<'a> VisitMut for AliasSubstitutor<'a> {
    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        if let syn::Type::Path(tp) = ty {
            if tp.qself.is_none() && tp.path.segments.len() == 1 {
                let seg = &tp.path.segments[0];
                if matches!(seg.arguments, syn::PathArguments::None) {
                    if let Some(replacement) = self.subs.get(seg.ident.to_string().as_str()) {
                        *ty = (*replacement).clone();
                        return;
                    }
                }
            }
        }
        syn::visit_mut::visit_type_mut(self, ty);
    }
}
