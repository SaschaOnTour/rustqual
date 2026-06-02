//! Tests for call-site collection + canonical resolution. Split into focused
//! sub-files (each ≤ the SRP file-length cap); shared imports, the `FileCtx`
//! fixture, and the parse/load/calls_in helpers live here and reach the
//! sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::calls::*;
pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::*;
pub(super) use crate::adapters::shared::use_tree::{gather_alias_map, AliasMap, ScopedAliasMap};
pub(super) use std::collections::{HashMap, HashSet};

mod basic;
mod inference_fallback;
mod receiver_tracking;
mod resolution_misc;
mod turbofish;

pub(super) fn parse_file(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse file")
}

pub(super) fn parse_type(src: &str) -> syn::Type {
    syn::parse_str(src).expect("parse type")
}

/// Build a context from a full file source plus the name of the fn whose
/// body we want to analyse. Picks up the fn's signature + alias map
/// automatically.
pub(super) struct FileCtx {
    pub(super) file: syn::File,
    pub(super) alias_map: AliasMap,
    pub(super) aliases_per_scope: ScopedAliasMap,
    pub(super) local_symbols: HashSet<String>,
    pub(super) local_decl_scopes: HashMap<String, Vec<Vec<String>>>,
    pub(super) crate_root_modules: HashSet<String>,
}

impl FileCtx {
    pub(super) fn file_scope<'a>(&'a self, importing_file: &'a str) -> FileScope<'a> {
        FileScope {
            path: importing_file,
            alias_map: &self.alias_map,
            aliases_per_scope: &self.aliases_per_scope,
            local_symbols: &self.local_symbols,
            local_decl_scopes: &self.local_decl_scopes,
            crate_root_modules: &self.crate_root_modules,
            workspace_module_paths: None,
        }
    }
}

pub(super) fn load(src: &str) -> FileCtx {
    let file = parse_file(src);
    let alias_map = gather_alias_map(&file);
    let local_symbols = collect_local_symbols(&file);
    // Single-file unit tests don't populate the scoped overlays —
    // they're left empty so the resolver falls back to flat behaviour.
    FileCtx {
        file,
        alias_map,
        aliases_per_scope: ScopedAliasMap::new(),
        local_symbols,
        local_decl_scopes: HashMap::new(),
        crate_root_modules: HashSet::new(),
    }
}

pub(super) fn load_with_roots(src: &str, roots: &[&str]) -> FileCtx {
    let mut fctx = load(src);
    fctx.crate_root_modules = roots.iter().map(|s| s.to_string()).collect();
    fctx
}

/// Convenience — rebuild crate_root_modules from a slice of pseudo-file
/// paths (same shape `build_call_graph` sees) so tests match the real
/// pipeline's derivation.
pub(super) fn roots_from_paths(paths: &[&str]) -> HashSet<String> {
    let fake: Vec<(&str, &syn::File)> = Vec::new();
    let _ = fake;
    let dummy = parse_file("");
    let refs: Vec<(&str, &syn::File)> = paths.iter().map(|p| (*p, &dummy)).collect();
    collect_crate_root_modules(&refs)
}

pub(super) fn find_fn<'a>(file: &'a syn::File, name: &str) -> &'a syn::ItemFn {
    file.items
        .iter()
        .find_map(|i| match i {
            syn::Item::Fn(f) if f.sig.ident == name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fn {name} not found"))
}

pub(super) fn impl_self_ty_name(item_impl: &syn::ItemImpl) -> Option<String> {
    match item_impl.self_ty.as_ref() {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

pub(super) fn find_impl_fn<'a>(
    file: &'a syn::File,
    type_name: &str,
    fn_name: &str,
) -> (&'a syn::ItemImpl, &'a syn::ImplItemFn) {
    file.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Impl(i) if impl_self_ty_name(i).as_deref() == Some(type_name) => Some(i),
            _ => None,
        })
        .find_map(|item_impl| {
            item_impl.items.iter().find_map(|it| match it {
                syn::ImplItem::Fn(f) if f.sig.ident == fn_name => Some((item_impl, f)),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("impl {type_name}::{fn_name} not found"))
}

pub(super) fn sig_params(sig: &syn::Signature) -> Vec<(String, &syn::Type)> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pt) => {
                let name = match pt.pat.as_ref() {
                    syn::Pat::Ident(pi) => pi.ident.to_string(),
                    _ => return None,
                };
                Some((name, pt.ty.as_ref()))
            }
            _ => None,
        })
        .collect()
}

pub(super) fn ctx_for_fn<'a>(
    fctx: &'a FileCtx,
    file_scope: &'a FileScope<'a>,
    fn_name: &str,
) -> FnContext<'a> {
    let f = find_fn(&fctx.file, fn_name);
    FnContext {
        file: file_scope,
        mod_stack: &[],
        body: &f.block,
        signature_params: sig_params(&f.sig),
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: None,
        workspace_files: None,
        reexports: None,
    }
}

/// Load `src`, resolve `fn_name`'s body as if imported into `importing_file`,
/// and return its canonical call set — the shared arrange across most tests
/// (load → file_scope → ctx_for_fn → collect_canonical_calls).
pub(super) fn calls_in(src: &str, importing_file: &str, fn_name: &str) -> HashSet<String> {
    let fctx = load(src);
    let fs = fctx.file_scope(importing_file);
    let ctx = ctx_for_fn(&fctx, &fs, fn_name);
    collect_canonical_calls(&ctx)
}

pub(super) fn calls_in_roots(
    src: &str,
    roots: &[&str],
    importing_file: &str,
    fn_name: &str,
) -> HashSet<String> {
    let fctx = load_with_roots(src, roots);
    let fs = fctx.file_scope(importing_file);
    let ctx = ctx_for_fn(&fctx, &fs, fn_name);
    collect_canonical_calls(&ctx)
}

pub(super) fn calls_in_impl_method(
    src: &str,
    importing_file: &str,
    type_name: &str,
    fn_name: &str,
) -> HashSet<String> {
    let fctx = load(src);
    let fs = fctx.file_scope(importing_file);
    let (item, f) = find_impl_fn(&fctx.file, type_name, fn_name);
    let ctx = FnContext {
        file: &fs,
        mod_stack: &[],
        body: &f.block,
        signature_params: sig_params(&f.sig),
        generic_params: std::collections::HashMap::new(),
        self_type: canonical_of_impl_self(item),
        workspace_index: None,
        workspace_files: None,
        reexports: None,
    };
    collect_canonical_calls(&ctx)
}

pub(super) fn canonical_of_impl_self(item: &syn::ItemImpl) -> Option<Vec<String>> {
    if let syn::Type::Path(p) = item.self_ty.as_ref() {
        Some(
            p.path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect(),
        )
    } else {
        None
    }
}
