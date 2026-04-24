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

use super::bindings::canonicalise_type_segments;
use super::calls::{collect_canonical_calls, FnContext};
use super::type_infer::{build_workspace_type_index, WorkspaceTypeIndex};
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::shared::cfg_test::has_cfg_test;
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
}

impl CallGraph {
    fn new() -> Self {
        Self {
            forward: HashMap::new(),
            reverse: HashMap::new(),
            layer_of: HashMap::new(),
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

    fn add_edge(&mut self, caller: &str, callee: &str) {
        self.forward
            .entry(caller.to_string())
            .or_default()
            .insert(callee.to_string());
        self.reverse
            .entry(callee.to_string())
            .or_default()
            .insert(caller.to_string());
    }

    fn add_node(&mut self, canonical: &str) {
        self.forward.entry(canonical.to_string()).or_default();
    }
}

/// Shared canonical-name builder used by both Check A and Check B.
/// The format matches what the graph stores as node keys so lookups
/// via `graph.forward` / `graph.reverse` / `graph.node_file` work.
pub(crate) fn canonical_name_for_pub_fn(info: &super::pub_fns::PubFnInfo<'_>) -> String {
    canonical_fn_name(&info.file, info.self_type.as_deref(), &info.fn_name)
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
fn canonical_fn_name(file: &str, self_type: Option<&[String]>, fn_name: &str) -> String {
    let mut segs: Vec<String> = Vec::new();
    match self_type {
        Some(impl_segs) if is_crate_rooted(impl_segs) => {
            segs.extend(impl_segs.iter().cloned());
        }
        Some(impl_segs) => {
            segs.push("crate".to_string());
            segs.extend(file_to_module_segments(file));
            segs.extend(impl_segs.iter().cloned());
        }
        None => {
            segs.push("crate".to_string());
            segs.extend(file_to_module_segments(file));
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

/// Collect the names of top-level items declared in this file — fns,
/// mods, types, consts, statics. The call collector uses this set to
/// resolve unqualified calls like `helper()` (without a `use`
/// statement) into `crate::<file_module>::helper`, instead of bare-name
/// dead-ends that disconnect local delegation chains.
pub(crate) fn collect_local_symbols(ast: &syn::File) -> HashSet<String> {
    ast.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Fn(f) => Some(f.sig.ident.to_string()),
            syn::Item::Mod(m) => Some(m.ident.to_string()),
            syn::Item::Struct(s) => Some(s.ident.to_string()),
            syn::Item::Enum(e) => Some(e.ident.to_string()),
            syn::Item::Union(u) => Some(u.ident.to_string()),
            syn::Item::Trait(t) => Some(t.ident.to_string()),
            syn::Item::Type(t) => Some(t.ident.to_string()),
            syn::Item::Const(c) => Some(c.ident.to_string()),
            syn::Item::Static(s) => Some(s.ident.to_string()),
            _ => None,
        })
        .collect()
}

/// Extract `(name, &Type)` pairs for every typed positional parameter
/// of a fn signature. Shared by pub-fn collection and graph-build since
/// both need the same `FnContext::signature_params` shape.
/// Framework-extractor patterns like `fn h(State(db): State<Db>)`
/// contribute `("db", State<Db>)` — the outer type still goes through
/// `resolve_type`, which peels the transparent wrapper to reach `Db`
/// when `State` is configured in `transparent_wrappers`.
pub(crate) fn extract_signature_params(sig: &syn::Signature) -> Vec<(String, &syn::Type)> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pt) => {
                param_name_from_pat(pt.pat.as_ref()).map(|n| (n, pt.ty.as_ref()))
            }
            _ => None,
        })
        .collect()
}

/// Pull the bound identifier out of a fn-parameter pattern. Supports
/// `Pat::Ident` (the 99% case) and single-ident `Pat::TupleStruct`
/// destructuring (framework extractors: `State(db)`, `Extension(ext)`,
/// `Path(p)`, `Json(body)`, `Data(ctx)`). Returns `None` for deeper
/// destructuring that the resolver can't express yet.
/// Operation: pattern peel.
fn param_name_from_pat(pat: &syn::Pat) -> Option<String> {
    match pat {
        syn::Pat::Ident(pi) => Some(pi.ident.to_string()),
        syn::Pat::TupleStruct(ts) if ts.elems.len() == 1 => {
            if let syn::Pat::Ident(pi) = &ts.elems[0] {
                return Some(pi.ident.to_string());
            }
            None
        }
        _ => None,
    }
}

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
pub(crate) fn resolve_impl_self_type(
    self_ty: &syn::Type,
    alias_map: &HashMap<String, Vec<String>>,
    local_symbols: &HashSet<String>,
    crate_root_modules: &HashSet<String>,
    importing_file: &str,
) -> Option<Vec<String>> {
    let raw = impl_self_ty_segments(self_ty)?;
    Some(
        canonicalise_type_segments(
            &raw,
            alias_map,
            local_symbols,
            crate_root_modules,
            importing_file,
        )
        .unwrap_or(raw),
    )
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
    // Pre-build the workspace type index so `collect_canonical_calls`
    // can run shallow inference on complex method-call receivers.
    let type_index = build_workspace_type_index(
        files,
        aliases_per_file,
        cfg_test_files,
        &crate_root_modules,
        transparent_wrappers,
    );
    let mut graph = CallGraph::new();
    for (path, ast) in files {
        if cfg_test_files.contains(*path) {
            continue;
        }
        let Some(alias_map) = aliases_per_file.get(*path) else {
            continue;
        };
        let local_symbols = collect_local_symbols(ast);
        let mut collector = FileFnCollector {
            path,
            alias_map,
            local_symbols: &local_symbols,
            crate_root_modules: &crate_root_modules,
            type_index: &type_index,
            impl_type_stack: Vec::new(),
            graph: &mut graph,
        };
        collector.visit_file(ast);
    }
    populate_layer_cache(&mut graph, layers);
    graph
}

/// Pre-compute `layer_of_crate_path` for every canonical that appears
/// in the graph (as source or sink). Hot-path BFS in Check A + Check B
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

struct FileFnCollector<'a> {
    path: &'a str,
    alias_map: &'a HashMap<String, Vec<String>>,
    local_symbols: &'a HashSet<String>,
    crate_root_modules: &'a HashSet<String>,
    type_index: &'a WorkspaceTypeIndex,
    /// Stack of enclosing impl blocks' resolved self-types. `None`
    /// marks an unresolved self-type (trait object, `&T`, tuple) whose
    /// methods we must not record — their canonical would collapse to
    /// `crate::<file>::method` and collide with free fns.
    impl_type_stack: Vec<Option<Vec<String>>>,
    graph: &'a mut CallGraph,
}

impl<'a> FileFnCollector<'a> {
    fn record_fn<'ast>(
        &mut self,
        fn_name: &str,
        sig: &'ast syn::Signature,
        body: &'ast syn::Block,
    ) {
        let self_type = match self.impl_type_stack.last() {
            // Free fn (no enclosing impl).
            None => None,
            // Resolved impl — use its canonical self-type.
            Some(Some(segs)) => Some(segs.clone()),
            // Unresolved impl (trait object / reference receiver) —
            // don't record; see `resolve_impl_self_type`'s doc.
            Some(None) => return,
        };
        let canonical = canonical_fn_name(self.path, self_type.as_deref(), fn_name);
        let ctx = FnContext {
            body,
            signature_params: extract_signature_params(sig),
            self_type,
            alias_map: self.alias_map,
            local_symbols: self.local_symbols,
            crate_root_modules: self.crate_root_modules,
            importing_file: self.path,
            workspace_index: Some(self.type_index),
        };
        let calls = collect_canonical_calls(&ctx);
        self.graph.add_node(&canonical);
        for callee in calls {
            self.graph.add_edge(&canonical, &callee);
        }
    }
}

impl<'a, 'ast> Visit<'ast> for FileFnCollector<'a> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        let name = node.sig.ident.to_string();
        self.record_fn(&name, &node.sig, &node.block);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        // Canonicalise the impl's self-type through the file's alias
        // map so `use crate::app::Session; impl Session { ... }` and
        // `impl Session { ... }` in `src/app/session.rs` both produce
        // the same `crate::app::Session` prefix the call collector sees
        // from receiver-tracked method calls. `None` means the
        // self-type isn't a plain path (trait object, `&T`, tuple) —
        // `record_fn` skips method recording for those impls.
        let resolved = resolve_impl_self_type(
            &node.self_ty,
            self.alias_map,
            self.local_symbols,
            self.crate_root_modules,
            self.path,
        );
        self.impl_type_stack.push(resolved);
        syn::visit::visit_item_impl(self, node);
        self.impl_type_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        let name = node.sig.ident.to_string();
        self.record_fn(&name, &node.sig, &node.block);
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        // Skip inline `#[cfg(test)] mod tests { ... }` blocks entirely.
        // Their fns are test-only and must not pollute the call graph
        // (Check B could otherwise count a test as adapter coverage).
        if has_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }
}
