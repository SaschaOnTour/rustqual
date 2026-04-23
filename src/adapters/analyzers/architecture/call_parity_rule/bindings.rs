//! Receiver-type binding extraction — the lookup side of the
//! `session.search(...)` → `crate::…::Session::search` resolution.
//!
//! Two entry points: `canonical_from_type` resolves a `syn::Type` to a
//! canonical path (stripping `&`, `Box`, `Arc`, `Rc`, `Cow` wrappers),
//! and `extract_let_binding` turns a `syn::Local` into a
//! `(name, canonical)` pair, preferring an explicit `let s: T =` annotation
//! over constructor-inference from `let s = T::new()`.

use crate::adapters::analyzers::architecture::forbidden_rule::resolve_to_crate_absolute;
use std::collections::HashMap;

/// Infer a canonical type-path from a `syn::Type`, stripping common
/// wrappers (`&T`, `&mut T`, `Box<T>`, `Arc<T>`, `Rc<T>`, `Cow<'_, T>`).
/// Returns `None` for unresolvable types (trait objects, generics,
/// external types without alias).
pub(super) fn canonical_from_type(
    ty: &syn::Type,
    alias_map: &HashMap<String, Vec<String>>,
    importing_file: &str,
) -> Option<Vec<String>> {
    let inner = strip_wrappers(ty);
    match inner {
        syn::Type::Path(tp) => {
            let segments: Vec<String> = tp
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            canonicalise_type_segments(&segments, alias_map, importing_file)
        }
        _ => None,
    }
}

/// Peel references and stdlib ownership wrappers until we hit something
/// else. `Arc<Rc<&Inner>>` → `Inner`. Conservative — only the well-known
/// wrapper names are stripped so generics we don't understand remain as-is.
// qual:recursive
fn strip_wrappers(ty: &syn::Type) -> &syn::Type {
    match ty {
        syn::Type::Reference(r) => strip_wrappers(&r.elem),
        syn::Type::Paren(p) => strip_wrappers(&p.elem),
        syn::Type::Path(tp) => {
            let Some(last) = tp.path.segments.last() else {
                return ty;
            };
            let name = last.ident.to_string();
            if !matches!(name.as_str(), "Box" | "Arc" | "Rc" | "Cow") {
                return ty;
            }
            let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
                return ty;
            };
            for arg in &args.args {
                if let syn::GenericArgument::Type(inner) = arg {
                    return strip_wrappers(inner);
                }
            }
            ty
        }
        _ => ty,
    }
}

/// Resolve a type-path segment list into a canonical `[crate, …]` path,
/// applying `crate/self/super` module normalisation and alias-map lookup.
/// Returns `None` for unresolvable paths (external crates, unaliased types).
pub(super) fn canonicalise_type_segments(
    segments: &[String],
    alias_map: &HashMap<String, Vec<String>>,
    importing_file: &str,
) -> Option<Vec<String>> {
    if segments.is_empty() {
        return None;
    }
    if matches!(segments[0].as_str(), "crate" | "self" | "super") {
        let resolved = resolve_to_crate_absolute(importing_file, segments)?;
        let mut full = vec!["crate".to_string()];
        full.extend(resolved);
        return Some(full);
    }
    if let Some(alias) = alias_map.get(&segments[0]) {
        let mut full = alias.clone();
        full.extend_from_slice(&segments[1..]);
        return Some(full);
    }
    None
}

/// Extract a `(name, canonical_type_path)` pair from a `let` statement.
/// Prefers an explicit type annotation (`let s: T = …`) over constructor
/// inference from the initializer (`let s = T::new()`).
pub(super) fn extract_let_binding(
    local: &syn::Local,
    alias_map: &HashMap<String, Vec<String>>,
    importing_file: &str,
) -> Option<(String, Vec<String>)> {
    let (name, annotated_ty) = extract_pat_name_and_type(&local.pat)?;
    if let Some(ty) = annotated_ty {
        if let Some(canonical) = canonical_from_type(ty, alias_map, importing_file) {
            return Some((name, canonical));
        }
    }
    let init = local.init.as_ref()?;
    let canonical = binding_type_from_init(&init.expr, alias_map, importing_file)?;
    Some((name, canonical))
}

/// Strip `Pat::Type` layers to get `(ident_name, Some(&Type))`, or
/// `(ident_name, None)` for plain `Pat::Ident`. Tuple / struct / ref
/// patterns yield `None` (MVP skips them).
fn extract_pat_name_and_type(pat: &syn::Pat) -> Option<(String, Option<&syn::Type>)> {
    match pat {
        syn::Pat::Ident(pi) => Some((pi.ident.to_string(), None)),
        syn::Pat::Type(pt) => {
            if let syn::Pat::Ident(pi) = pt.pat.as_ref() {
                Some((pi.ident.to_string(), Some(pt.ty.as_ref())))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Infer the canonical type of a binding from its initializer when no
/// explicit annotation is present. Unwraps `?`, `.await`, and parens,
/// then looks for a constructor pattern `Type::ctor(args)` and maps the
/// prefix to a canonical path via alias_map / resolve_to_crate_absolute.
fn binding_type_from_init(
    expr: &syn::Expr,
    alias_map: &HashMap<String, Vec<String>>,
    importing_file: &str,
) -> Option<Vec<String>> {
    let mut cur = expr;
    loop {
        match cur {
            syn::Expr::Try(t) => cur = &t.expr,
            syn::Expr::Await(a) => cur = &a.base,
            syn::Expr::Paren(p) => cur = &p.expr,
            _ => break,
        }
    }
    let call = match cur {
        syn::Expr::Call(c) => c,
        _ => return None,
    };
    let path = match call.func.as_ref() {
        syn::Expr::Path(p) => &p.path,
        _ => return None,
    };
    let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if segments.len() < 2 {
        return None;
    }
    let type_segments = &segments[..segments.len() - 1];
    canonicalise_type_segments(type_segments, alias_map, importing_file)
}
