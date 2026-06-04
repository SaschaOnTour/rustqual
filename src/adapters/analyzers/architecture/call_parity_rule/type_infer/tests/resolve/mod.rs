//! Unit tests for `syn::Type` → `CanonicalType` conversion (the `resolve`
//! module). Split into focused sub-files (each ≤ the SRP file-length cap);
//! shared imports + the `parse_type`/`ctx`/`resolve_*` helpers live here and
//! reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::canonical::CanonicalType;
pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::resolve::{
    resolve_type, ResolveContext,
};
pub(super) use crate::adapters::shared::use_tree::{AliasTarget, ScopedAliasMap};
pub(super) use std::collections::{HashMap, HashSet};

mod part_a;
mod part_b;

pub(super) fn parse_type(src: &str) -> syn::Type {
    syn::parse_str(src).expect("parse type")
}

pub(super) fn ctx<'a>(file: &'a FileScope<'a>) -> ResolveContext<'a> {
    ResolveContext {
        file,
        mod_stack: &[],
        type_aliases: None,
        transparent_wrappers: None,
        workspace_files: None,
        alias_param_subs: None,
        generic_params: None,
        reexports: None,
    }
}

/// Resolve `ty_src` in a `FileScope` with the given path, local symbols, and
/// crate-root modules (empty alias / scoped-alias / decl-scope maps, no
/// workspace module paths). Covers the common test shape; tests that need a
/// populated alias map or scoped aliases build the `FileScope` inline.
pub(super) fn resolve_in(
    ty_src: &str,
    path: &str,
    locals: &[&str],
    roots: &[&str],
) -> CanonicalType {
    let alias_map = HashMap::new();
    let local: HashSet<String> = locals.iter().map(|s| s.to_string()).collect();
    let root_set: HashSet<String> = roots.iter().map(|s| s.to_string()).collect();
    resolve_type(
        &parse_type(ty_src),
        &ctx(&FileScope {
            path,
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &root_set,
            workspace_module_paths: None,
        }),
    )
}

/// Resolve a type with a populated `use`-alias map: each `(name, target)`
/// becomes `alias_map[name] = AliasTarget::relative(target)`.
pub(super) fn resolve_aliased(
    ty_src: &str,
    path: &str,
    aliases: &[(&str, &[&str])],
    locals: &[&str],
) -> CanonicalType {
    let mut alias_map = HashMap::new();
    for (name, target) in aliases {
        alias_map.insert(
            name.to_string(),
            AliasTarget::relative(target.iter().map(|s| s.to_string()).collect()),
        );
    }
    let local: HashSet<String> = locals.iter().map(|s| s.to_string()).collect();
    let roots = HashSet::new();
    resolve_type(
        &parse_type(ty_src),
        &ctx(&FileScope {
            path,
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
            workspace_module_paths: None,
        }),
    )
}

/// Resolve a type inside a fn body that declares a single unbounded generic
/// param `param` (turbofish index 0) — the param shadows same-named symbols.
pub(super) fn resolve_with_unbounded_param(
    ty_src: &str,
    path: &str,
    locals: &[&str],
    param: &str,
) -> CanonicalType {
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::ParamInfo;
    let alias_map = HashMap::new();
    let local: HashSet<String> = locals.iter().map(|s| s.to_string()).collect();
    let roots = HashSet::new();
    let file_scope = FileScope {
        path,
        alias_map: &alias_map,
        aliases_per_scope: &ScopedAliasMap::new(),
        local_symbols: &local,
        local_decl_scopes: &HashMap::new(),
        crate_root_modules: &roots,
        workspace_module_paths: None,
    };
    let mut generics: HashMap<String, ParamInfo> = HashMap::new();
    generics.insert(
        param.to_string(),
        ParamInfo {
            bounds: vec![],
            turbofish_index: Some(0),
        },
    );
    resolve_type(
        &parse_type(ty_src),
        &ResolveContext {
            file: &file_scope,
            mod_stack: &[],
            type_aliases: None,
            transparent_wrappers: None,
            workspace_files: None,
            alias_param_subs: None,
            generic_params: Some(&generics),
            reexports: None,
        },
    )
}
