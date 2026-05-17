//! Receiver-type binding extraction — the lookup side of the
//! `session.search(...)` → `crate::…::Session::search` resolution.
//!
//! Two entry points: `canonical_from_type` resolves a `syn::Type` to a
//! canonical path (stripping `&`, `Box`, `Arc`, `Rc`, `Cow` wrappers),
//! and `extract_let_binding` turns a `syn::Local` into a
//! `(name, canonical)` pair, preferring an explicit `let s: T =` annotation
//! over constructor-inference from `let s = T::new()`.

use super::local_symbols::{scope_for_local, FileScope};
use crate::adapters::analyzers::architecture::forbidden_rule::{
    file_to_module_segments, resolve_to_crate_absolute, resolve_to_crate_absolute_in,
};
use crate::adapters::shared::use_tree::{AliasMap, AliasTarget, ScopedAliasMap};
use std::collections::{HashMap, HashSet};

/// Infer a canonical type-path from a `syn::Type`, stripping common
/// wrappers (`&T`, `&mut T`, `Box<T>`, `Arc<T>`, `Rc<T>`, `Cow<'_, T>`).
/// Returns `None` for unresolvable types (trait objects, generics,
/// external types without alias).
pub(super) fn canonical_from_type(
    ty: &syn::Type,
    alias_map: &AliasMap,
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
            // Use-site gate: extern-root `::ext::Foo` doesn't match
            // workspace symbols, so short-circuit before the
            // workspace canonicaliser.
            if tp.path.leading_colon.is_some() {
                return None;
            }
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

/// Bundled inputs for canonical-type-path resolution. `file` carries
/// per-file lookup tables; `mod_stack` is per-call-site; `reexports`
/// is workspace-global — when `Some`, the gate substitutes
/// re-exported prefixes back to DECL canonicals. `None` falls back
/// to legacy v1.2.4 file-alias-only behaviour for test fixtures.
pub(crate) struct CanonScope<'a> {
    pub file: &'a FileScope<'a>,
    pub mod_stack: &'a [String],
    pub reexports: Option<&'a super::reexports::ReexportMap>,
}

/// Legacy helper for callers without a full `FileScope` (unit-test
/// fixtures, the `canonical_from_type` adapter). Builds an empty
/// `ScopedAliasMap` / `local_decl_scopes` overlay so the scope-aware
/// primitive falls back to flat behaviour automatically. Callers
/// dealing with user-written `syn::Path` must check
/// `path.leading_colon.is_some()` themselves BEFORE invoking this
/// helper — the flat-map shape doesn't carry the leading-colon bit.
pub(super) fn canonicalise_type_segments(
    segments: &[String],
    alias_map: &AliasMap,
    local_symbols: &HashSet<String>,
    crate_root_modules: &HashSet<String>,
    importing_file: &str,
) -> Option<Vec<String>> {
    let empty_scoped = ScopedAliasMap::new();
    let empty_decls = HashMap::new();
    let file = FileScope {
        path: importing_file,
        alias_map,
        aliases_per_scope: &empty_scoped,
        local_symbols,
        local_decl_scopes: &empty_decls,
        crate_root_modules,
        workspace_module_paths: None,
    };
    canonicalise_type_segments_in_scope(
        segments,
        &CanonScope {
            file: &file,
            mod_stack: &[],
            reexports: None,
        },
    )
}

// qual:api
/// Path canonicalisation gate: resolve a path's segments to a
/// workspace canonical, respecting Rust 2018+ `::Foo` extern-root
/// semantics. THE single gate for "user-written `syn::Path` →
/// workspace canonical" — every site that observes a path written by
/// the user MUST route through this helper, NOT call
/// `canonicalise_type_segments_in_scope` directly.
///
/// The rule applies to both expression-side AND declaration-side
/// paths. Originally we treated declarations as exempt ("internal
/// code never writes leading-colon paths"), but in practice
/// declaration sites routinely carry `::ext` prefixes too:
/// `use ::ext::Foo;`, `pub use ::ext::Bar;`,
/// `impl ::ext::Trait for X { … }`, `fn f<Q: ::ext::Bound>(...)`,
/// `impl X { } where Self: ::ext::Marker`. Each is a declaration that
/// must NOT promote `ext` to a workspace canonical when a same-named
/// workspace module exists. The call sites that consume these
/// declarations (`pub_fns_visibility::is_visible`,
/// `pub_fns_alias_chain::resolve_alias_target_canonical`,
/// `workspace_index::traits::resolve_trait_path`,
/// `workspace_graph::resolve_impl_self_type`, etc.) all read
/// `path.leading_colon.is_some()` and pass it here.
///
/// `leading_colon_set` short-circuits the workspace lookup: an
/// absolute path (`::Foo::bar`) is explicit extern-crate syntax (we
/// don't model extern crates as workspace symbols) and must NOT
/// match a same-named workspace `Foo`. The primitive
/// `canonicalise_type_segments_in_scope` (which this wraps) can't
/// see the leading colon — segment lists carry no `::` info — so
/// without this gate, `::Foo::bar` mis-resolves to
/// `crate::...::Foo::bar` when a workspace `Foo` exists.
///
/// The few remaining direct callers of the primitive
/// `canonicalise_type_segments_in_scope` (the legacy flat-map
/// adapter `canonicalise_type_segments`, and
/// `signature_params::canonicalise_bounds`) are themselves gated
/// upstream: they receive segment lists from helpers that have
/// already filtered out leading-colon paths inline. New code MUST
/// NOT bypass the gate.
/// Operation: leading-colon gate + delegate.
pub(crate) fn canonicalise_workspace_path(
    segments: &[String],
    leading_colon_set: bool,
    scope: &CanonScope<'_>,
) -> Option<Vec<String>> {
    if leading_colon_set {
        return None;
    }
    let resolved = canonicalise_type_segments_in_scope(segments, scope)?;
    Some(super::reexports::apply_reexport_substitution(
        resolved,
        scope.reexports,
    ))
}

/// Resolve a type-path segment list against `scope`.
///
/// **Return shape is not uniform** — callers that expect "workspace
/// canonical or None" must filter results explicitly:
/// - `Some(["crate", …])` — the path resolves into the workspace.
/// - `Some(other)` — the path is recognised but extern-rooted (an
///   alias whose `use` had a leading `::`, or an unaliased non-crate
///   first segment). Returned as-is so downstream lookups treat
///   `ext::Foo::…` as the extern symbol it actually is, not a
///   `crate::ext::Foo::…` phantom.
/// - `None` — the segment list is empty, has unknown idents that
///   don't match any alias / local / crate-root, or its alias chain
///   couldn't normalise (e.g. unresolved `self::`/`super::`).
///
/// In practice the only consumer that meaningfully observes the
/// extern-rooted `Some(other)` branch is the receiver-type binding
/// inference (`canonical_from_type`), which uses the returned segments
/// as a binding's type identity — extern bindings produce no workspace
/// edge downstream, so the contract works. New callers that need
/// strict "workspace canonical XOR None" semantics should add a
/// thin wrapper that filters `result.first() == Some("crate")`.
///
/// **PRIMITIVE — DO NOT CALL FROM USE-SITE CONTEXTS.** This function
/// has no `leading_colon` awareness; the segment-list interface
/// cannot carry the `::` prefix. Callers handling a `syn::Path` from
/// user code (call expressions, type references in fn bodies, trait
/// bounds, `let`-binding annotations, `impl Trait for X` paths,
/// `pub use` targets, etc.) MUST go through
/// `canonicalise_workspace_path`, passing `path.leading_colon.is_some()`
/// so an explicit absolute path (`::ext::Foo`) doesn't false-match a
/// same-named workspace symbol. The only remaining direct caller is
/// the legacy flat-map adapter `canonicalise_type_segments`, whose
/// own callers gate `leading_colon` inline before invoking it.
pub(crate) fn canonicalise_type_segments_in_scope(
    segments: &[String],
    scope: &CanonScope<'_>,
) -> Option<Vec<String>> {
    if segments.is_empty() {
        return None;
    }
    let file = scope.file;
    if matches!(segments[0].as_str(), "crate" | "self" | "super") {
        let resolved = resolve_to_crate_absolute_in(file.path, scope.mod_stack, segments)?;
        let mut full = vec!["crate".to_string()];
        full.extend(resolved);
        return Some(full);
    }
    if let Some(alias) = lookup_alias(scope, &segments[0]) {
        let mut full = alias.segments.to_vec();
        full.extend_from_slice(&segments[1..]);
        return normalize_after_alias(full, alias.absolute_root, scope);
    }
    if file.local_symbols.contains(&segments[0]) {
        if let Some(mod_path) =
            scope_for_local(file.local_decl_scopes, &segments[0], scope.mod_stack)
        {
            let mut full = vec!["crate".to_string()];
            full.extend(file_to_module_segments(file.path));
            full.extend(mod_path.iter().cloned());
            full.extend_from_slice(segments);
            return Some(full);
        }
    }
    if file.crate_root_modules.contains(&segments[0]) {
        let mut full = vec!["crate".to_string()];
        full.extend_from_slice(segments);
        return Some(full);
    }
    None
}

/// Resolve `name` against the alias map for exactly the current
/// `mod_stack`. Rust `use` items are module-local — child mods don't
/// inherit parents — so this looks up only at the current scope. When
/// the scoped overlay has no entry for `mod_stack` (legacy / unit-test
/// callers), falls back to the flat `alias_map`.
fn lookup_alias<'a>(scope: &'a CanonScope<'a>, name: &str) -> Option<&'a AliasTarget> {
    if let Some(map) = scope.file.aliases_per_scope.get(scope.mod_stack) {
        return map.get(name);
    }
    scope.file.alias_map.get(name)
}

/// After alias-map substitution, re-run `self` / `super` normalisation
/// (relative to `mod_stack` inside `importing_file`, so an alias
/// declared inside an inline mod resolves its `self`/`super` against
/// that mod) and prepend `crate` for Rust 2018+ absolute imports.
///
/// `absolute_root` is the leading-colon bit of the originating `use`
/// item: `use ::ext::Foo as Local;` carries `absolute_root=true` and
/// means "this is an extern path, NOT the workspace's `ext`." On that
/// flag we MUST NOT apply workspace canonicalisation (sibling-submodule
/// promotion, crate-root prepending) — that's the exact bug the
/// in-tree alias-map leading-colon drift caused before v1.2.4.
fn normalize_after_alias(
    expanded: Vec<String>,
    absolute_root: bool,
    scope: &CanonScope<'_>,
) -> Option<Vec<String>> {
    if absolute_root {
        // Extern-rooted alias (`use ::ext::Foo as Local;`). The path
        // is intentionally NOT a workspace canonical — return it as-is
        // so downstream lookups treat `ext::Foo::…` as the extern
        // symbol it actually is, not a `crate::ext::Foo::…` phantom.
        return Some(expanded);
    }
    let file = scope.file;
    let mod_stack = scope.mod_stack;
    match expanded.first().map(|s| s.as_str()) {
        Some("self") | Some("super") => {
            let resolved = resolve_to_crate_absolute_in(file.path, mod_stack, &expanded)?;
            let mut full = vec!["crate".to_string()];
            full.extend(resolved);
            Some(full)
        }
        Some("crate") => Some(expanded),
        // Rust 2018+ resolution priority: local sibling submodule wins
        // over crate-root module of the same name. `use foo::bar` inside
        // `crate::application` resolves to `crate::application::foo::bar`
        // when that submodule exists, even if `crate::foo` also exists.
        // Reversing this order misroutes valid local imports.
        Some(first)
            if is_workspace_submodule(file.workspace_module_paths, file.path, mod_stack, first) =>
        {
            let mut with_self = vec!["self".to_string()];
            with_self.extend(expanded);
            let resolved = resolve_to_crate_absolute_in(file.path, mod_stack, &with_self)?;
            let mut full = vec!["crate".to_string()];
            full.extend(resolved);
            Some(full)
        }
        Some(first) if file.crate_root_modules.contains(first) => {
            let mut full = vec!["crate".to_string()];
            full.extend(expanded);
            Some(full)
        }
        _ => Some(expanded),
    }
}

/// True when `[importing_file's mod path, mod_stack…, first]` is a
/// known module path in the workspace — i.e. `first` names a real
/// sibling submodule of the current module rather than an extern
/// crate. Returns `false` when no workspace_module_paths is available
/// (test fixtures without full workspace setup) so legacy behaviour
/// is preserved.
fn is_workspace_submodule(
    workspace_module_paths: Option<&HashSet<Vec<String>>>,
    importing_file: &str,
    mod_stack: &[String],
    first: &str,
) -> bool {
    let Some(paths) = workspace_module_paths else {
        return false;
    };
    let mut candidate =
        crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments(
            importing_file,
        );
    candidate.extend_from_slice(mod_stack);
    candidate.push(first.to_string());
    paths.contains(&candidate)
}

pub(super) fn normalize_alias_expansion(
    expanded: Vec<String>,
    absolute_root: bool,
    scope: &CanonScope<'_>,
) -> Option<Vec<String>> {
    normalize_after_alias(expanded, absolute_root, scope)
}

/// Extract a `(name, canonical_type_path)` pair from a `let` statement.
/// Prefers an explicit type annotation (`let s: T = …`) over constructor
/// inference from the initializer (`let s = T::new()`).
pub(super) fn extract_let_binding(
    local: &syn::Local,
    alias_map: &AliasMap,
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
    alias_map: &AliasMap,
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
    // Use-site gate: `let x = ::ext::Foo::ctor()` extern-root path
    // must not false-match a workspace `Foo`.
    if path.leading_colon.is_some() {
        return None;
    }
    canonicalise_type_segments(
        type_segments,
        alias_map,
        local_symbols,
        crate_root_modules,
        importing_file,
    )
}
