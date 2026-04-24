//! Shared test helpers for Check A / Check B integration-style tests.

use crate::adapters::analyzers::architecture::call_parity_rule::check_a::check_no_delegation;
use crate::adapters::analyzers::architecture::call_parity_rule::check_b::check_missing_adapter;
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::collect_pub_fns_by_layer;
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::build_call_graph;
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::analyzers::architecture::MatchLocation;
use crate::adapters::shared::use_tree::gather_alias_map;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::{HashMap, HashSet};

/// In-memory workspace built from `(path, source)` pairs.
pub(super) struct Workspace {
    pub files: Vec<(String, String, syn::File)>,
    pub aliases_per_file: HashMap<String, HashMap<String, Vec<String>>>,
}

pub(super) fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse")
}

pub(super) fn globset(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).unwrap());
    }
    b.build().unwrap()
}

/// Build a workspace + pre-compute alias maps per file.
pub(super) fn build_workspace(entries: &[(&str, &str)]) -> Workspace {
    let mut files = Vec::new();
    let mut aliases_per_file = HashMap::new();
    for (path, src) in entries {
        let ast = parse(src);
        let alias_map = gather_alias_map(&ast);
        aliases_per_file.insert(path.to_string(), alias_map);
        files.push((path.to_string(), src.to_string(), ast));
    }
    Workspace {
        files,
        aliases_per_file,
    }
}

/// Borrow the parsed files as `(&path, &syn::File)` — the shape the
/// graph + pub-fn collectors accept. Tied to `ws`'s lifetime.
pub(super) fn borrowed_files(ws: &Workspace) -> Vec<(&str, &syn::File)> {
    ws.files.iter().map(|(p, _, f)| (p.as_str(), f)).collect()
}

/// Which call-parity check to run against the pre-built graph. A tiny
/// enum tag keeps `run_check_a` / `run_check_b` from sharing identical
/// body statements (DRY-004 fragment-match) without the HRTB lifetime
/// gymnastics that a `FnOnce` closure would require.
pub(super) enum Check {
    A,
    B,
}

/// Run a call-parity check end-to-end against a workspace. Integration:
/// builds pub-fns + graph, then dispatches on `which`.
pub(super) fn run_check(
    which: Check,
    ws: &Workspace,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
    cfg_test: &HashSet<String>,
) -> Vec<MatchLocation> {
    let borrowed = borrowed_files(ws);
    let pub_fns = collect_pub_fns_by_layer(&borrowed, &ws.aliases_per_file, layers, cfg_test);
    let empty_wrappers = HashSet::new();
    let graph = build_call_graph(
        &borrowed,
        &ws.aliases_per_file,
        cfg_test,
        layers,
        &empty_wrappers,
    );
    match which {
        Check::A => check_no_delegation(&pub_fns, &graph, layers, cp),
        Check::B => check_missing_adapter(&pub_fns, &graph, layers, cp),
    }
}

/// Run Check A (adapter-must-delegate). Operation: thin wrapper.
pub(super) fn run_check_a(
    ws: &Workspace,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
    cfg_test: &HashSet<String>,
) -> Vec<MatchLocation> {
    run_check(Check::A, ws, layers, cp, cfg_test)
}

/// Run Check B (target-must-be-reached). Operation: thin wrapper.
pub(super) fn run_check_b(
    ws: &Workspace,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
    cfg_test: &HashSet<String>,
) -> Vec<MatchLocation> {
    run_check(Check::B, ws, layers, cp, cfg_test)
}

/// An empty `cfg_test` HashSet — convenience for callers that don't
/// exercise test-file filtering.
pub(super) fn empty_cfg_test() -> HashSet<String> {
    HashSet::new()
}
