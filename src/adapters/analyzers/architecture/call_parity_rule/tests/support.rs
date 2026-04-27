//! Shared test helpers for the call-parity integration-style tests
//! (Checks A/B/C/D, touchpoints, pub-fn collection).

use crate::adapters::analyzers::architecture::call_parity_rule::build_handler_touchpoints;
use crate::adapters::analyzers::architecture::call_parity_rule::check_a::check_no_delegation;
use crate::adapters::analyzers::architecture::call_parity_rule::check_b::check_missing_adapter;
use crate::adapters::analyzers::architecture::call_parity_rule::check_c::check_multi_touchpoint;
use crate::adapters::analyzers::architecture::call_parity_rule::check_d::check_multiplicity_mismatch;
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::collect_pub_fns_by_layer;
use crate::adapters::analyzers::architecture::call_parity_rule::touchpoints::compute_touchpoints;
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
    build_call_graph, canonical_name_for_pub_fn,
};
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

/// Three-layer test fixture: application + cli + mcp.
/// Operation: LayerDefinitions construction.
pub(super) fn three_layer() -> LayerDefinitions {
    LayerDefinitions::new(
        vec![
            "application".to_string(),
            "cli".to_string(),
            "mcp".to_string(),
        ],
        vec![
            ("application".to_string(), globset(&["src/application/**"])),
            ("cli".to_string(), globset(&["src/cli/**"])),
            ("mcp".to_string(), globset(&["src/mcp/**"])),
        ],
    )
}

/// `[architecture.call_parity]` configured for cli + mcp adapters,
/// application as target, with a tunable `call_depth`. The most common
/// shape across tests; per-file helpers customize only when they need
/// different adapters or exclude_targets.
/// Operation: struct literal construction.
pub(super) fn cli_mcp_config(call_depth: usize) -> CompiledCallParity {
    CompiledCallParity {
        adapters: vec!["cli".to_string(), "mcp".to_string()],
        target: "application".to_string(),
        call_depth,
        exclude_targets: GlobSet::empty(),
        transparent_wrappers: HashSet::new(),
        transparent_macros: HashSet::new(),
        single_touchpoint: crate::config::architecture::SingleTouchpointMode::default(),
    }
}

/// Four-layer test fixture: application + cli + mcp + rest.
/// Operation: LayerDefinitions construction.
pub(super) fn four_layer() -> LayerDefinitions {
    LayerDefinitions::new(
        vec![
            "application".to_string(),
            "cli".to_string(),
            "mcp".to_string(),
            "rest".to_string(),
        ],
        vec![
            ("application".to_string(), globset(&["src/application/**"])),
            ("cli".to_string(), globset(&["src/cli/**"])),
            ("mcp".to_string(), globset(&["src/mcp/**"])),
            ("rest".to_string(), globset(&["src/rest/**"])),
        ],
    )
}

/// Which call-parity check to run against the pre-built graph. A tiny
/// enum tag keeps `run_check_a` / `run_check_b` / `run_check_c` /
/// `run_check_d` from sharing identical body statements (DRY-004
/// fragment-match) without the HRTB lifetime gymnastics that a
/// `FnOnce` closure would require.
pub(super) enum Check {
    A,
    B,
    C,
    D,
}

/// Build the workspace's pub-fns map and call graph. Integration:
/// shared by `run_check` and `compute_touchpoints_for`.
fn build_pub_fns_and_graph<'ws>(
    ws: &'ws Workspace,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
    cfg_test: &HashSet<String>,
) -> (
    HashMap<
        String,
        Vec<crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::PubFnInfo<'ws>>,
    >,
    crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::CallGraph,
) {
    let borrowed = borrowed_files(ws);
    let pub_fns = collect_pub_fns_by_layer(
        &borrowed,
        &ws.aliases_per_file,
        layers,
        cfg_test,
        &cp.transparent_wrappers,
    );
    let graph = build_call_graph(
        &borrowed,
        &ws.aliases_per_file,
        cfg_test,
        layers,
        &cp.transparent_wrappers,
    );
    (pub_fns, graph)
}

/// Run a call-parity check end-to-end against a workspace. Integration:
/// builds pub-fns + graph, then dispatches on `which`.
// qual:allow(dry) — match-dispatch over Check kinds; each arm targets a
// distinct check fn with a different signature.
pub(super) fn run_check(
    which: Check,
    ws: &Workspace,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
    cfg_test: &HashSet<String>,
) -> Vec<MatchLocation> {
    let (pub_fns, graph) = build_pub_fns_and_graph(ws, layers, cp, cfg_test);
    let touchpoints = build_handler_touchpoints(&pub_fns, &graph, cp);
    match which {
        Check::A => check_no_delegation(&pub_fns, &touchpoints, cp),
        Check::B => check_missing_adapter(&pub_fns, &graph, &touchpoints, cp),
        Check::C => check_multi_touchpoint(&pub_fns, &touchpoints, cp),
        Check::D => check_multiplicity_mismatch(&pub_fns, &touchpoints, cp),
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

/// Run Check C (single-touchpoint). Operation: thin wrapper.
pub(super) fn run_check_c(
    ws: &Workspace,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
    cfg_test: &HashSet<String>,
) -> Vec<MatchLocation> {
    run_check(Check::C, ws, layers, cp, cfg_test)
}

/// Run Check D (multiplicity-must-match). Operation: thin wrapper.
pub(super) fn run_check_d(
    ws: &Workspace,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
    cfg_test: &HashSet<String>,
) -> Vec<MatchLocation> {
    run_check(Check::D, ws, layers, cp, cfg_test)
}

/// An empty `cfg_test` HashSet — convenience for callers that don't
/// exercise test-file filtering.
pub(super) fn empty_cfg_test() -> HashSet<String> {
    HashSet::new()
}

/// Build pub-fns + graph and compute touchpoints for one named handler.
/// Integration: builds the workspace graph, finds the handler by its
/// short fn_name, then delegates to `compute_touchpoints`.
pub(super) fn compute_touchpoints_for(
    ws: &Workspace,
    layers: &LayerDefinitions,
    cp: &CompiledCallParity,
    handler_fn_name: &str,
    cfg_test: &HashSet<String>,
) -> HashSet<String> {
    let (pub_fns, graph) = build_pub_fns_and_graph(ws, layers, cp, cfg_test);
    let info = pub_fns
        .values()
        .flatten()
        .find(|i| i.fn_name == handler_fn_name)
        .unwrap_or_else(|| panic!("handler `{handler_fn_name}` not found in pub_fns"));
    let canonical = canonical_name_for_pub_fn(info);
    compute_touchpoints(&canonical, &graph, &cp.target, cp.call_depth)
}
