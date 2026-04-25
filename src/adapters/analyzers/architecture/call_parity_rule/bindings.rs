//! Receiver-type binding extraction — the lookup side of the
//! `session.search(...)` → `crate::…::Session::search` resolution.
//!
//! Two entry points: `canonical_from_type` resolves a `syn::Type` to a
//! canonical path (stripping `&`, `Box`, `Arc`, `Rc`, `Cow` wrappers),
//! and `extract_let_binding` turns a `syn::Local` into a
//! `(name, canonical)` pair, preferring an explicit `let s: T =` annotation
//! over constructor-inference from `let s = T::new()`.

use super::local_symbols::scope_for_local;
use crate::adapters::analyzers::architecture::forbidden_rule::{
    file_to_module_segments, resolve_to_crate_absolute, resolve_to_crate_absolute_in,
};
use crate::adapters::shared::use_tree::ScopedAliasMap;
use std::collections::{HashMap, HashSet};

/// Infer a canonical type-path from a `syn::Type`, stripping common
/// wrappers (`&T`, `&mut T`, `Box<T>`, `Arc<T>`, `Rc<T>`, `Cow<'_, T>`).
/// Returns `None` for unresolvable types (trait objects, generics,
/// external types without alias).
pub(super) fn canonical_from_type(
    ty: &syn::Type,
    alias_map: &HashMap<String, Vec<String>>,
    local_symbols: &HashSet<String>,
    crate_root_modules: &HashSet<String>,
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
            canonicalise_type_segments(
                &segments,
                alias_map,
                local_symbols,
                crate_root_modules,
                importing_file,
            )
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

/// Bundled inputs for canonical-type-path resolution. The same-file
/// fallback walks `mod_stack` outward against `local_decl_scopes` /
/// `aliases_per_scope` to pick the closest enclosing declaration of a
/// single-ident name.
pub(crate) struct CanonScope<'a> {
    /// Top-level (file-scope) `use` aliases. Used as the legacy fallback
    /// when `aliases_per_scope` is `None`.
    pub alias_map: &'a HashMap<String, Vec<String>>,
    pub local_symbols: &'a HashSet<String>,
    pub crate_root_modules: &'a HashSet<String>,
    pub importing_file: &'a str,
    pub local_decl_scopes: Option<&'a HashMap<String, Vec<Vec<String>>>>,
    /// `mod_path → (name → canonical_path)` capturing each inline
    /// module's own `use` items. The empty `Vec` key holds top-level
    /// aliases. When `Some`, alias lookup walks `mod_stack` outward
    /// honouring per-mod scope; when `None`, the flat `alias_map` is
    /// used (legacy / unit-test path).
    pub aliases_per_scope: Option<&'a ScopedAliasMap>,
    pub mod_stack: &'a [String],
}

/// Legacy helper for callers that don't track mod scope (call collector,
/// the `bindings::canonical_from_type` adapter, tests). Delegates to the
/// scope-aware variant with empty `mod_stack` + `None` decl scopes —
/// behaviour unchanged. Operation: thin wrapper.
pub(super) fn canonicalise_type_segments(
    segments: &[String],
    alias_map: &HashMap<String, Vec<String>>,
    local_symbols: &HashSet<String>,
    crate_root_modules: &HashSet<String>,
    importing_file: &str,
) -> Option<Vec<String>> {
    canonicalise_type_segments_in_scope(
        segments,
        &CanonScope {
            alias_map,
            local_symbols,
            crate_root_modules,
            importing_file,
            local_decl_scopes: None,
            aliases_per_scope: None,
            mod_stack: &[],
        },
    )
}

/// Resolve a type-path segment list into a canonical `[crate, …]` path,
/// applying `crate/self/super` module normalisation, alias-map lookup,
/// and same-file fallback (mod-scope-aware when `scope.mod_stack` /
/// `scope.local_decl_scopes` are populated). Returns `None` for
/// unresolvable paths (external crates, unknown idents).
/// Operation: closure-hidden own calls keep IOSP clean.
pub(crate) fn canonicalise_type_segments_in_scope(
    segments: &[String],
    scope: &CanonScope<'_>,
) -> Option<Vec<String>> {
    if segments.is_empty() {
        return None;
    }
    let normalize =
        |full| normalize_after_alias(full, scope.importing_file, scope.crate_root_modules);
    if matches!(segments[0].as_str(), "crate" | "self" | "super") {
        let resolved =
            resolve_to_crate_absolute_in(scope.importing_file, scope.mod_stack, segments)?;
        let mut full = vec!["crate".to_string()];
        full.extend(resolved);
        return Some(full);
    }
    if let Some(alias) = lookup_alias(scope, &segments[0]) {
        let mut full = alias.to_vec();
        full.extend_from_slice(&segments[1..]);
        return normalize(full);
    }
    if scope.local_symbols.contains(&segments[0]) {
        let mod_path = scope_for_local(scope.local_decl_scopes, &segments[0], scope.mod_stack);
        let mut full = vec!["crate".to_string()];
        full.extend(file_to_module_segments(scope.importing_file));
        full.extend(mod_path.iter().cloned());
        full.extend_from_slice(segments);
        return Some(full);
    }
    if scope.crate_root_modules.contains(&segments[0]) {
        let mut full = vec!["crate".to_string()];
        full.extend_from_slice(segments);
        return Some(full);
    }
    None
}

/// Resolve `name` against the alias maps in scope. Walks `mod_stack`
/// outward through `aliases_per_scope` so a `use` declared inside an
/// inline mod shadows a same-name file-level alias. Falls back to the
/// flat `alias_map` when the scoped overlay isn't provided (legacy /
/// unit-test path).
fn lookup_alias<'a>(scope: &'a CanonScope<'a>, name: &str) -> Option<&'a [String]> {
    if let Some(per_scope) = scope.aliases_per_scope {
        for depth in (0..=scope.mod_stack.len()).rev() {
            let prefix = &scope.mod_stack[..depth];
            if let Some(map) = per_scope.get(prefix) {
                if let Some(path) = map.get(name) {
                    return Some(path.as_slice());
                }
            }
        }
        return None;
    }
    scope.alias_map.get(name).map(Vec::as_slice)
}

/// After alias-map substitution, re-run `self` / `super` normalisation
/// and prepend `crate` for Rust 2018+ absolute imports that resolve
/// into a known workspace root module (`use app::foo;` →
/// `crate::app::foo`). Without this, both forms leak non-crate-rooted
/// canonicals that never match graph nodes.
fn normalize_after_alias(
    expanded: Vec<String>,
    importing_file: &str,
    crate_root_modules: &HashSet<String>,
) -> Option<Vec<String>> {
    match expanded.first().map(|s| s.as_str()) {
        Some("self") | Some("super") => {
            let resolved = resolve_to_crate_absolute(importing_file, &expanded)?;
            let mut full = vec!["crate".to_string()];
            full.extend(resolved);
            Some(full)
        }
        Some("crate") => Some(expanded),
        Some(first) if crate_root_modules.contains(first) => {
            let mut full = vec!["crate".to_string()];
            full.extend(expanded);
            Some(full)
        }
        _ => Some(expanded),
    }
}

pub(super) fn normalize_alias_expansion(
    expanded: Vec<String>,
    importing_file: &str,
    crate_root_modules: &HashSet<String>,
) -> Option<Vec<String>> {
    normalize_after_alias(expanded, importing_file, crate_root_modules)
}

/// Extract a `(name, canonical_type_path)` pair from a `let` statement.
/// Prefers an explicit type annotation (`let s: T = …`) over constructor
/// inference from the initializer (`let s = T::new()`).
pub(super) fn extract_let_binding(
    local: &syn::Local,
    alias_map: &HashMap<String, Vec<String>>,
    local_symbols: &HashSet<String>,
    crate_root_modules: &HashSet<String>,
    importing_file: &str,
) -> Option<(String, Vec<String>)> {
    let (name, annotated_ty) = extract_pat_name_and_type(&local.pat)?;
    if let Some(ty) = annotated_ty {
        if let Some(canonical) = canonical_from_type(
            ty,
            alias_map,
            local_symbols,
            crate_root_modules,
            importing_file,
        ) {
            return Some((name, canonical));
        }
    }
    let init = local.init.as_ref()?;
    let canonical = binding_type_from_init(
        &init.expr,
        alias_map,
        local_symbols,
        crate_root_modules,
        importing_file,
    )?;
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
    local_symbols: &HashSet<String>,
    crate_root_modules: &HashSet<String>,
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
    canonicalise_type_segments(
        type_segments,
        alias_map,
        local_symbols,
        crate_root_modules,
        importing_file,
    )
}
