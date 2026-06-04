//! Integration tests for `WorkspaceTypeIndex` building — struct-field,
//! method-return, and free-fn-return collection across single- and multi-file
//! workspaces, turbofish/generic substitution, inline-mod + trait indexing,
//! and leading-colon disambiguation. Split into focused sub-files (each ≤ the
//! SRP file-length cap); shared imports, the `WsFixture`/`ScopedCalls`
//! fixtures, and the build/calls helpers live here and reach the sub-modules
//! via `use super::*`.

pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::{
    build_workspace_files_map, collect_local_symbols_scoped, LocalSymbols, WorkspaceFilesInputs,
};
pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    build_workspace_type_index, CanonicalType, WorkspaceIndexInputs, WorkspaceTypeIndex,
};
pub(super) use crate::adapters::shared::use_tree::{
    gather_alias_map, gather_alias_map_scoped, AliasMap, ScopedAliasMap,
};
pub(super) use std::collections::{HashMap, HashSet};

// Pipeline-wiring imports for `calls_from` / `calls_from_scoped` (kept at module
// level so the helper bodies stay act-only, not import-laden).
use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
    collect_canonical_calls, FnContext,
};
use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::{
    extract_signature_params, item_canonical_generics,
};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
    collect_crate_root_modules, collect_local_symbols,
};

mod empty_and_struct_fields;
mod inline_mod_and_traits;
mod leading_colon_and_disambiguation;
mod method_and_fn_returns;
mod turbofish_and_generics;
mod visitor_walks;

pub(super) fn parse_file(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse file")
}

pub(super) struct WsFixture {
    pub(super) parsed: Vec<(String, syn::File)>,
    pub(super) aliases: HashMap<String, AliasMap>,
    pub(super) aliases_scoped: HashMap<String, ScopedAliasMap>,
    pub(super) local_symbols: HashMap<String, LocalSymbols>,
}

pub(super) fn fixture(entries: &[(&str, &str)]) -> WsFixture {
    let mut parsed = Vec::new();
    let mut aliases = HashMap::new();
    let mut aliases_scoped = HashMap::new();
    let mut local_symbols = HashMap::new();
    for (path, src) in entries {
        let ast = parse_file(src);
        aliases.insert(path.to_string(), gather_alias_map(&ast));
        aliases_scoped.insert(path.to_string(), gather_alias_map_scoped(&ast));
        local_symbols.insert(path.to_string(), collect_local_symbols_scoped(&ast));
        parsed.push((path.to_string(), ast));
    }
    WsFixture {
        parsed,
        aliases,
        aliases_scoped,
        local_symbols,
    }
}

pub(super) fn borrowed(f: &WsFixture) -> Vec<(&str, &syn::File)> {
    f.parsed.iter().map(|(p, a)| (p.as_str(), a)).collect()
}

pub(super) fn crate_roots(paths: &[&str]) -> HashSet<String> {
    paths
        .iter()
        .filter_map(|p| {
            let rest = p.strip_prefix("src/")?;
            let first = rest.split('/').next()?;
            let name = first.strip_suffix(".rs").unwrap_or(first);
            if matches!(name, "lib" | "main") {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

/// Build a `WorkspaceTypeIndex` for the common test case — empty cfg-test
/// and transparent-wrapper sets, no re-exports, given crate roots. Tests
/// that need a non-empty set call the builders directly.
pub(super) fn build_index(
    files: &[(&str, &syn::File)],
    fix: &WsFixture,
    roots: &HashSet<String>,
) -> WorkspaceTypeIndex {
    let cfg_test = HashSet::new();
    let wraps = HashSet::new();
    let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
        files,
        cfg_test_files: &cfg_test,
        aliases_per_file: &fix.aliases,
        aliases_scoped_per_file: &fix.aliases_scoped,
        local_symbols_per_file: &fix.local_symbols,
        crate_root_modules: roots,
        workspace_module_paths: None,
    });
    build_workspace_type_index(&WorkspaceIndexInputs {
        files,
        workspace_files: &workspace_files,
        cfg_test_files: &cfg_test,
        transparent_wrappers: &wraps,
        reexports: None,
    })
}

/// One-shot for the common test shape: parse `entries` into a fixture and
/// build its `WorkspaceTypeIndex` with the given crate-root paths (empty
/// slice = no roots). Use this when the test only needs the index; tests
/// that also reuse the fixture/borrowed files build it step by step.
pub(super) fn index_for(entries: &[(&str, &str)], root_paths: &[&str]) -> WorkspaceTypeIndex {
    let fix = fixture(entries);
    let borrowed_files = borrowed(&fix);
    build_index(&borrowed_files, &fix, &crate_roots(root_paths))
}

/// Run the canonical-call inference pipeline over the `fn_index`-th item of a
/// parsed `use_site_src`, against `workspace_index`, and return the resolved
/// call set. Centralises the FileScope/FnContext wiring that the
/// turbofish/inference tests would otherwise each repeat verbatim.
pub(super) fn calls_from(
    workspace_index: &WorkspaceTypeIndex,
    use_site_src: &str,
    fn_index: usize,
) -> std::collections::HashSet<String> {
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::extract_signature_params;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let use_site = parse_file(use_site_src);
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/cli/use_site.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/cli/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let (body, sig) = match &use_site.items[fn_index] {
        syn::Item::Fn(item_fn) => (&item_fn.block, &item_fn.sig),
        _ => panic!("expected fn item at index {fn_index} of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: extract_signature_params(sig),
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(workspace_index),
        workspace_files: None,
        reexports: None,
    };
    collect_canonical_calls(&ctx)
}

/// Inputs for [`calls_from_scoped`]. The leading-colon / generic-bound /
/// dyn-trait disambiguation tests need a custom file path, extra crate-root
/// files (so a same-named workspace trait/symbol is canonicalisable — the
/// false-positive trigger), and either the fn's item-level generics or its
/// signature params — none of which the plain `calls_from` exposes.
pub(super) struct ScopedCalls<'a> {
    pub(super) workspace_index: &'a WorkspaceTypeIndex,
    pub(super) use_site_src: &'a str,
    pub(super) path: &'a str,
    pub(super) fn_index: usize,
    pub(super) extra_root_files: &'a [(&'a str, &'a str)],
    pub(super) with_generics: bool,
}

/// Run the FileScope → FnContext → `collect_canonical_calls` pipeline with a
/// caller-controlled scope. Collapses the inline FileScope/FnContext arrange
/// the leading-colon disambiguation tests would otherwise repeat.
pub(super) fn calls_from_scoped(input: ScopedCalls) -> std::collections::HashSet<String> {
    let use_site = parse_file(input.use_site_src);
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let extra_parsed: Vec<(&str, syn::File)> = input
        .extra_root_files
        .iter()
        .map(|(p, s)| (*p, parse_file(s)))
        .collect();
    let mut root_inputs: Vec<(&str, &syn::File)> = vec![(input.path, &use_site)];
    root_inputs.extend(extra_parsed.iter().map(|(p, f)| (*p, f)));
    let crate_roots_set = collect_crate_root_modules(&root_inputs);
    let file_scope = FileScope {
        path: input.path,
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let (body, sig) = match &use_site.items[input.fn_index] {
        syn::Item::Fn(item_fn) => (&item_fn.block, &item_fn.sig),
        _ => panic!("expected fn item at index {} of use_site", input.fn_index),
    };
    let (generic_params, signature_params) = if input.with_generics {
        (
            item_canonical_generics(&sig.generics, &file_scope, &[], None),
            vec![],
        )
    } else {
        (
            std::collections::HashMap::new(),
            extract_signature_params(sig),
        )
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params,
        generic_params,
        self_type: None,
        workspace_index: Some(input.workspace_index),
        workspace_files: None,
        reexports: None,
    };
    collect_canonical_calls(&ctx)
}
