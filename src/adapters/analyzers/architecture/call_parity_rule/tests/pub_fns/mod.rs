//! Tests for `collect_pub_fns_by_layer` — workspace-wide pub-fn enumeration
//! grouped by architecture layer. Split into focused sub-files (each ≤ the
//! SRP file-length cap); shared imports, the parse/alias/layer helpers, and
//! the `PubFnCase` table type live here and reach the sub-modules via
//! `use super::*`.

pub(super) use super::support::three_layer;
pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::WorkspaceLookup;
pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::{
    collect_pub_fns_by_layer, PubFnInfo, PubFnInputs,
};
pub(super) use crate::adapters::shared::use_tree::{gather_alias_map, AliasMap};
pub(super) use std::collections::{HashMap, HashSet};

mod classification;
mod deprecated;
mod free_and_groups;
mod private_mods;
mod reexport_aliases;

/// Build an `aliases_per_file` map from a workspace slice — mirrors
/// what the call-parity entry point computes.
pub(super) fn aliases_from_files(files: &[(&str, &syn::File)]) -> HashMap<String, AliasMap> {
    files
        .iter()
        .map(|(p, f)| (p.to_string(), gather_alias_map(f)))
        .collect()
}

pub(super) fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse file")
}

pub(super) fn names_for_layer<'ast>(
    by_layer: &std::collections::HashMap<String, Vec<PubFnInfo<'ast>>>,
    layer: &str,
) -> HashSet<String> {
    by_layer
        .get(layer)
        .map(|fns| fns.iter().map(|f| f.fn_name.clone()).collect())
        .unwrap_or_default()
}

/// Collect pub fns by layer for the common test case — empty wrapper,
/// promoted-attribute, and workspace-lookup sets. Tests that need a
/// non-empty set call `collect_pub_fns_by_layer` directly.
pub(super) fn pub_fns_by_layer<'ast>(
    files: &[(&'ast str, &'ast syn::File)],
) -> HashMap<String, Vec<PubFnInfo<'ast>>> {
    let aliases = aliases_from_files(files);
    collect_pub_fns_by_layer(PubFnInputs {
        files,
        aliases_per_file: &aliases,
        layers: &three_layer(),
        transparent_wrappers: &HashSet::new(),
        promoted_attributes: &HashSet::new(),
        workspace: &crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::WorkspaceLookup {
            cfg_test_files: &HashSet::new(),
            crate_root_modules: &HashSet::new(),
            workspace_module_paths: &HashSet::new(),
        },
    })
}

/// Parse each `(path, src)` into a workspace, collect pub fns for `layer`
/// honouring `wrappers` as transparent wrappers, and return that layer's fn
/// names. Covers the single-file, multi-file, and user-wrapper test shapes
/// with the default (empty) cfg-test / crate-root / workspace-path sets.
pub(super) fn layer_names_w(
    file_srcs: &[(&str, &str)],
    layer: &str,
    wrappers: &[&str],
) -> HashSet<String> {
    let parsed: Vec<syn::File> = file_srcs.iter().map(|(_, s)| parse(s)).collect();
    let files: Vec<(&str, &syn::File)> = file_srcs
        .iter()
        .zip(&parsed)
        .map(|((p, _), f)| (*p, f))
        .collect();
    let aliases = aliases_from_files(&files);
    let wrapper_set: HashSet<String> = wrappers.iter().map(|w| w.to_string()).collect();
    let by_layer = collect_pub_fns_by_layer(PubFnInputs {
        files: &files,
        aliases_per_file: &aliases,
        layers: &three_layer(),
        transparent_wrappers: &wrapper_set,
        promoted_attributes: &HashSet::new(),
        workspace: &WorkspaceLookup {
            cfg_test_files: &HashSet::new(),
            crate_root_modules: &HashSet::new(),
            workspace_module_paths: &HashSet::new(),
        },
    });
    names_for_layer(&by_layer, layer)
}

// (label, files[(path, src)], layer, transparent wrappers, present names, absent names)

pub(super) type PubFnCase = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);
