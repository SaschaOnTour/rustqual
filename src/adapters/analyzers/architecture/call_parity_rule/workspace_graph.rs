//! Workspace-wide canonical call graph shared by Check A and Check B.
//!
//! Walks every fn (free + impl, pub + private) across non-cfg-test files,
//! turns each into a canonical name (`crate::<file_module>::<fn>` or
//! `crate::<file_module>::<Type>::<method>`), and records:
//!
//! - `forward[caller] = {callees}` — what each fn calls.
//! - `reverse[callee] = {callers}` — inverse of `forward`, pre-built so
//!   Check B's BFS doesn't pay O(N) lookup per step.
//!
//! Layer membership of a given node is derived on demand via
//! `LayerDefinitions::layer_of_crate_path(canonical)` rather than cached
//! per-file. A per-canonical file map would silently overwrite on name
//! collisions (trait-impl vs inherent `Type::method` with identical
//! canonical), and layer derivation from the canonical path is both
//! deterministic and consistent with Check A's forward walk.
//!
//! Private fns are needed because adapters commonly delegate through
//! file-local helpers — walking only pub fns would under-count delegation
//! chains and trigger false positives in Check A.

use super::anchor_index::{build_anchor_info, is_anchor_target_capability, AnchorInfo};
use super::bindings::{canonicalise_type_segments_in_scope, CanonScope};
use super::file_fn_collector::FileFnCollector;
use super::type_infer::{build_workspace_type_index, WorkspaceIndexInputs, WorkspaceTypeIndex};
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::shared::use_tree::gather_alias_map_scoped;
use crate::adapters::shared::use_tree::ScopedAliasMap;
use std::collections::{HashMap, HashSet, VecDeque};
use syn::visit::Visit;

/// Pre-built workspace call graph. Built once per analysis run.
pub(crate) struct CallGraph {
    /// canonical_caller → set of canonical callees it emits.
    pub forward: HashMap<String, HashSet<String>>,
    /// canonical_callee → set of canonical callers (inverse of `forward`).
    pub reverse: HashMap<String, HashSet<String>>,
    /// canonical → resolved layer name (or `None` for external / bare /
    /// unresolvable targets). Pre-populated at graph-build time for every
    /// canonical that appears as a source or sink, so the BFS loops in
    /// Check A / Check B stay O(N) instead of paying a glob probe per
    /// visited node.
    pub layer_of: HashMap<String, Option<String>>,
    /// Synthetic trait-method anchors emitted by `dyn Trait.method()`
    /// dispatch — `<Trait>::<method>` for every `(trait, method)` where
    /// the trait is workspace-known and the method is declared on it.
    /// Each `AnchorInfo` carries impl-layer set, impl-method canonicals,
    /// the trait's declaring layer, and the source location, so the
    /// touchpoint walker, Check B/D capability rule, the
    /// concrete-impl-skip filter, and finding-location rendering can
    /// all share one consistent data source.
    pub trait_method_anchors: HashMap<String, AnchorInfo>,
}

impl CallGraph {
    fn new() -> Self {
        Self {
            forward: HashMap::new(),
            reverse: HashMap::new(),
            layer_of: HashMap::new(),
            trait_method_anchors: HashMap::new(),
        }
    }

    /// Look up the cached layer for a canonical. Returns `Some(layer)`
    /// only when the canonical resolves to a workspace file in one of
    /// the configured layers; returns `None` for external / bare /
    /// unresolvable targets AND for canonicals that weren't seen during
    /// graph build (the caller is then responsible for treating the
    /// absence as "layer unknown", same as `layer_of_crate_path`).
    pub fn layer_of(&self, canonical: &str) -> Option<&str> {
        self.layer_of.get(canonical).and_then(Option::as_deref)
    }

    /// Iterate over `(anchor_canonical, AnchorInfo)` pairs for every
    /// trait-method anchor that passes the unified target-capability
    /// rule. These are **target capabilities** — Check B/D enumerate
    /// them alongside concrete `pub_fns_by_layer[target]` entries,
    /// and the boundary walker registers them as touchpoints. The
    /// walker and Check B/D MUST share the same rule (via
    /// `is_anchor_target_capability`) — otherwise an adapter could
    /// reach an anchor the walker accepts but Check B doesn't
    /// enumerate, producing silent false-negatives, or vice versa.
    /// Returning the `&AnchorInfo` alongside the canonical avoids the
    /// defensive re-lookup callers would otherwise need.
    pub(crate) fn target_anchor_capabilities<'a>(
        &'a self,
        target_layer: &'a str,
        adapter_layers: &'a [String],
    ) -> impl Iterator<Item = (&'a str, &'a AnchorInfo)> + 'a {
        self.trait_method_anchors
            .iter()
            .filter(move |(_, info)| {
                is_anchor_target_capability(info, target_layer, adapter_layers)
            })
            .map(|(anchor, info)| (anchor.as_str(), info))
    }

    /// True iff `canonical` is the impl-method canonical of a trait
    /// whose anchor is enumerated as target capability under the same
    /// rules. Used by Check B/D to skip concrete trait-impl-method
    /// pub-fns that the anchor already covers — without this, an
    /// adapter that dispatches via `dyn Trait.method()` and reaches
    /// only the anchor would produce a false orphan finding for every
    /// concrete impl-method.
    pub(crate) fn is_anchor_backed_concrete(
        &self,
        canonical: &str,
        target_layer: &str,
        adapter_layers: &[String],
    ) -> bool {
        self.trait_method_anchors.values().any(|info| {
            is_anchor_target_capability(info, target_layer, adapter_layers)
                && info.impl_method_canonicals.contains(canonical)
        })
    }

    pub(super) fn add_edge(&mut self, caller: &str, callee: &str) {
        self.forward
            .entry(caller.to_string())
            .or_default()
            .insert(callee.to_string());
        self.reverse
            .entry(callee.to_string())
            .or_default()
            .insert(caller.to_string());
    }

    pub(super) fn add_node(&mut self, canonical: &str) {
        self.forward.entry(canonical.to_string()).or_default();
    }
}

/// Shared canonical-name builder used by both Check A and Check B.
/// The format matches what the graph stores as node keys so lookups
/// via `graph.forward` / `graph.reverse` / `graph.node_file` work.
pub(crate) fn canonical_name_for_pub_fn(info: &super::pub_fns::PubFnInfo<'_>) -> String {
    canonical_fn_name(
        &info.file,
        info.self_type.as_deref(),
        &info.mod_stack,
        &info.fn_name,
    )
}

/// Lower-level primitive for constructing canonical fn names. Shared
/// between `canonical_name_for_pub_fn` (which adapts a `PubFnInfo`) and
/// `FileFnCollector::canonical_name` (which composes segments during
/// the graph walk).
///
/// Qualified impl paths (`impl crate::foo::Bar { ... }`) are used as-is
/// — if the user already spelled out the canonical path in the impl
/// header, we must not prepend the file's module segments or we'd
/// produce `crate::<file_mod>::crate::foo::Bar::method`, which never
/// matches receiver-tracked method targets.
pub(super) fn canonical_fn_name(
    file: &str,
    self_type: Option<&[String]>,
    mod_stack: &[String],
    fn_name: &str,
) -> String {
    let mut segs: Vec<String> = Vec::new();
    match self_type {
        Some(impl_segs) if is_crate_rooted(impl_segs) => {
            segs.extend(impl_segs.iter().cloned());
        }
        Some(impl_segs) => {
            segs.push("crate".to_string());
            segs.extend(file_to_module_segments(file));
            segs.extend(mod_stack.iter().cloned());
            segs.extend(impl_segs.iter().cloned());
        }
        None => {
            segs.push("crate".to_string());
            segs.extend(file_to_module_segments(file));
            segs.extend(mod_stack.iter().cloned());
        }
    }
    segs.push(fn_name.to_string());
    segs.join("::")
}

fn is_crate_rooted(segments: &[String]) -> bool {
    segments.first().map(|s| s.as_str()) == Some("crate")
}

/// Collect the set of first-segment module names at the crate root.
/// Every `src/<name>.rs` / `src/<name>/**.rs` file contributes `<name>`.
/// Used so Rust 2018+ absolute imports (`use app::foo;` — no `crate::`
/// prefix) resolve to `crate::app::foo` instead of a bare `app::foo`
/// that never matches graph nodes.
pub(crate) fn collect_crate_root_modules(files: &[(&str, &syn::File)]) -> HashSet<String> {
    files
        .iter()
        .filter_map(|(path, _)| crate_root_module_of(path))
        .collect()
}

/// Extract the first module segment from a `src/...` path. Returns
/// `None` for `src/lib.rs` / `src/main.rs` (crate roots, not modules).
fn crate_root_module_of(path: &str) -> Option<String> {
    let rest = path.strip_prefix("src/")?;
    let first = rest.split('/').next()?;
    let name = first.strip_suffix(".rs").unwrap_or(first);
    if matches!(name, "lib" | "main") {
        return None;
    }
    Some(name.to_string())
}

pub(crate) use super::local_symbols::{
    build_workspace_files_map, collect_local_symbols, collect_local_symbols_scoped, FileScope,
    LocalSymbols,
};

/// Canonicalise an impl block's self-type through the same alias /
/// local-symbol / crate-root pipeline the call collector uses for type
/// bindings. Returns a crate-rooted segment list when the type resolves
/// (via `use`, same-file declaration, or absolute workspace module);
/// falls back to the raw identifiers only when the type path exists
/// but can't be canonicalised further.
///
/// Returns `None` for self-types we can't parse at all (trait objects,
/// references, tuples). Callers must skip method recording in that
/// case — pushing an empty segment list would cause `canonical_fn_name`
/// to drop the type segment entirely and collide with free fns.
///
/// Known limitation: alias-chained self-types are not expanded. For
/// `pub type Public = private::Hidden; impl Public { fn op() {} }`,
/// the impl is keyed under `Public::op` but receiver-type inference
/// resolves `x: Public` callers to `Hidden::op`, leaving the edge
/// disconnected. A correct fix would chase `alias_chain` here and
/// register the method under every alias-equivalent canonical
/// (touching the `impl_stack` shape and the four call sites of
/// this function); deferred until a real workspace exhibits the bug.
pub(crate) fn resolve_impl_self_type(
    self_ty: &syn::Type,
    scope: &CanonScope<'_>,
) -> Option<Vec<String>> {
    let raw = impl_self_ty_segments(self_ty)?;
    Some(canonicalise_type_segments_in_scope(&raw, scope).unwrap_or(raw))
}

/// Flatten a `syn::Type::Path` to its segment identifiers — the shape
/// the call-parity rule uses to remember which impl block a method
/// lives in. Non-path types (trait objects, tuples) return `None`.
pub(crate) fn impl_self_ty_segments(self_ty: &syn::Type) -> Option<Vec<String>> {
    match self_ty {
        syn::Type::Path(p) => Some(
            p.path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect(),
        ),
        _ => None,
    }
}

/// Shared BFS scaffold used by both Check A (forward walk, target-layer
/// probe) and Check B (reverse walk, adapter-layer coverage). Keeps the
/// queue + visited invariants and the seed semantics in one place.
///
/// Marks nodes visited at enqueue time so each node is queued at most
/// once — in a DAG with many convergent edges, the naive "mark at
/// dequeue" pattern can queue the same node thousands of times.
pub(crate) struct WalkState {
    pub queue: VecDeque<(String, usize)>,
    pub visited: HashSet<String>,
}

impl WalkState {
    pub fn seeded(start: &str, direct: &HashSet<String>) -> Self {
        let mut visited = HashSet::new();
        visited.insert(start.to_string());
        let mut queue = VecDeque::new();
        for c in direct {
            if visited.insert(c.clone()) {
                queue.push_back((c.clone(), 1));
            }
        }
        Self { queue, visited }
    }

    pub fn enqueue_unvisited(&mut self, nodes: &HashSet<String>, depth: usize) {
        for c in nodes {
            if self.visited.insert(c.clone()) {
                self.queue.push_back((c.clone(), depth));
            }
        }
    }
}

// qual:api
/// Build the workspace call graph. Skips cfg-test files wholesale;
/// every fn in a non-test file contributes a node, and each of its
/// canonical calls (via `collect_canonical_calls`) becomes an edge.
/// `layers` is consumed to pre-compute the per-canonical layer cache
/// (see `CallGraph.layer_of`).
/// Integration: walks files + delegates per-fn canonical-call collection.
pub(crate) fn build_call_graph<'ast>(
    files: &[(&'ast str, &'ast syn::File)],
    aliases_per_file: &HashMap<String, HashMap<String, Vec<String>>>,
    cfg_test_files: &HashSet<String>,
    layers: &LayerDefinitions,
    transparent_wrappers: &HashSet<String>,
) -> CallGraph {
    let crate_root_modules = collect_crate_root_modules(files);
    // Pre-compute `LocalSymbols` + per-mod alias maps per file once and
    // reuse across the type-index passes + the graph collector.
    let local_symbols_per_file: HashMap<String, LocalSymbols> = files
        .iter()
        .filter(|(p, _)| !cfg_test_files.contains(*p))
        .map(|(path, ast)| (path.to_string(), collect_local_symbols_scoped(ast)))
        .collect();
    let aliases_scoped_per_file: HashMap<String, ScopedAliasMap> = files
        .iter()
        .filter(|(p, _)| !cfg_test_files.contains(*p))
        .map(|(path, ast)| (path.to_string(), gather_alias_map_scoped(ast)))
        .collect();
    let workspace_files = build_workspace_files_map(super::local_symbols::WorkspaceFilesInputs {
        files,
        cfg_test_files,
        aliases_per_file,
        aliases_scoped_per_file: &aliases_scoped_per_file,
        local_symbols_per_file: &local_symbols_per_file,
        crate_root_modules: &crate_root_modules,
    });
    let type_index = build_workspace_type_index(&WorkspaceIndexInputs {
        files,
        workspace_files: &workspace_files,
        cfg_test_files,
        transparent_wrappers,
    });
    let mut graph = CallGraph::new();
    for (path, ast) in files {
        let Some(file) = workspace_files.get(*path) else {
            continue;
        };
        let mut collector = FileFnCollector {
            file,
            workspace_files: &workspace_files,
            type_index: &type_index,
            impl_type_stack: Vec::new(),
            mod_stack: Vec::new(),
            graph: &mut graph,
        };
        collector.visit_file(ast);
    }
    populate_anchor_index(&mut graph, &type_index, layers);
    populate_layer_cache(&mut graph, layers);
    graph
}

/// Mirror `WorkspaceTypeIndex.trait_methods` + `trait_impls` into the
/// graph as synthetic anchors `<Trait>::<method>` with full
/// `AnchorInfo` metadata: impl-layer set (walker hot-path), impl-method
/// canonicals (Check B/D concrete-skip), declaring layer (peer-adapter
/// rejection + default-only-target rule), and source location
/// (anchor-finding rendering). Resolved eagerly at build time so the
/// hot-path stays O(1). Integration: delegate per-method anchor build.
fn populate_anchor_index(
    graph: &mut CallGraph,
    type_index: &WorkspaceTypeIndex,
    layers: &LayerDefinitions,
) {
    for (trait_canonical, methods) in type_index.trait_methods_iter() {
        let decl_layer = layers
            .layer_of_crate_path(trait_canonical)
            .map(String::from);
        for method in methods {
            let anchor = format!("{trait_canonical}::{method}");
            let info = build_anchor_info(type_index, layers, trait_canonical, method, &decl_layer);
            graph.trait_method_anchors.insert(anchor, info);
        }
    }
}

/// Pre-compute `layer_of_crate_path` for every canonical that appears
/// in the graph (as source or sink). Hot-path BFS in the call-parity checks
/// can then look up layers in O(1) instead of doing glob probes per
/// visited node — measured ~1.5s saved on rustqual's own source tree.
fn populate_layer_cache(graph: &mut CallGraph, layers: &LayerDefinitions) {
    let mut canonicals: HashSet<String> = graph.forward.keys().cloned().collect();
    for callees in graph.forward.values() {
        canonicals.extend(callees.iter().cloned());
    }
    for canonical in canonicals {
        let layer = layers.layer_of_crate_path(&canonical).map(String::from);
        graph.layer_of.insert(canonical, layer);
    }
}
