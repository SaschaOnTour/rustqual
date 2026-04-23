//! Workspace-wide canonical call graph shared by Check A and Check B.
//!
//! Walks every fn (free + impl, pub + private) across non-cfg-test files,
//! turns each into a canonical name (`crate::<file_module>::<fn>` or
//! `crate::<file_module>::<Type>::<method>`), and records:
//!
//! - `forward[caller] = {callees}` — what each fn calls.
//! - `node_file[fn] = path` — the file each canonical fn is declared in.
//! - `reverse[callee] = {callers}` — inverse of `forward`, pre-built so
//!   Check B's BFS doesn't pay O(N) lookup per step.
//!
//! Private fns are needed because adapters commonly delegate through
//! file-local helpers — walking only pub fns would under-count delegation
//! chains and trigger false positives in Check A.

use super::calls::{collect_canonical_calls, FnContext};
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use std::collections::{HashMap, HashSet, VecDeque};
use syn::visit::Visit;

/// Pre-built workspace call graph. Built once per analysis run.
pub(crate) struct CallGraph {
    /// canonical_caller → set of canonical callees it emits.
    pub forward: HashMap<String, HashSet<String>>,
    /// canonical_fn → file path where it is declared.
    pub node_file: HashMap<String, String>,
    /// canonical_callee → set of canonical callers (inverse of `forward`).
    pub reverse: HashMap<String, HashSet<String>>,
}

impl CallGraph {
    fn new() -> Self {
        Self {
            forward: HashMap::new(),
            node_file: HashMap::new(),
            reverse: HashMap::new(),
        }
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

    fn add_node(&mut self, canonical: &str, file: &str) {
        self.node_file
            .insert(canonical.to_string(), file.to_string());
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
fn canonical_fn_name(file: &str, self_type: Option<&[String]>, fn_name: &str) -> String {
    let mut segs: Vec<String> = vec!["crate".to_string()];
    segs.extend(file_to_module_segments(file));
    if let Some(impl_segs) = self_type {
        segs.extend(impl_segs.iter().cloned());
    }
    segs.push(fn_name.to_string());
    segs.join("::")
}

/// Extract `(name, &Type)` pairs for every typed positional parameter
/// of a fn signature. Shared by pub-fn collection and graph-build since
/// both need the same `FnContext::signature_params` shape.
pub(crate) fn extract_signature_params(sig: &syn::Signature) -> Vec<(String, &syn::Type)> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pt) => match pt.pat.as_ref() {
                syn::Pat::Ident(pi) => Some((pi.ident.to_string(), pt.ty.as_ref())),
                _ => None,
            },
            _ => None,
        })
        .collect()
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
pub(crate) struct WalkState {
    pub queue: VecDeque<(String, usize)>,
    pub visited: HashSet<String>,
}

impl WalkState {
    pub fn seeded(start: &str, direct: &HashSet<String>) -> Self {
        let mut visited = HashSet::new();
        visited.insert(start.to_string());
        Self {
            queue: direct.iter().map(|c| (c.clone(), 1)).collect(),
            visited,
        }
    }

    pub fn enqueue_unvisited(&mut self, nodes: &HashSet<String>, depth: usize) {
        for c in nodes {
            if !self.visited.contains(c) {
                self.queue.push_back((c.clone(), depth));
            }
        }
    }
}

// qual:api
/// Build the workspace call graph. Skips cfg-test files wholesale;
/// every fn in a non-test file contributes a node, and each of its
/// canonical calls (via `collect_canonical_calls`) becomes an edge.
/// Integration: walks files + delegates per-fn canonical-call collection.
pub(crate) fn build_call_graph<'ast>(
    files: &'ast [(String, String, &'ast syn::File)],
    aliases_per_file: &HashMap<String, HashMap<String, Vec<String>>>,
    cfg_test_files: &HashSet<String>,
) -> CallGraph {
    let mut graph = CallGraph::new();
    for (path, _src, ast) in files {
        if cfg_test_files.contains(path) {
            continue;
        }
        let Some(alias_map) = aliases_per_file.get(path) else {
            continue;
        };
        let mut collector = FileFnCollector {
            path,
            alias_map,
            impl_type_stack: Vec::new(),
            graph: &mut graph,
        };
        collector.visit_file(ast);
    }
    graph
}

struct FileFnCollector<'a> {
    path: &'a str,
    alias_map: &'a HashMap<String, Vec<String>>,
    impl_type_stack: Vec<Vec<String>>,
    graph: &'a mut CallGraph,
}

impl<'a> FileFnCollector<'a> {
    fn record_fn<'ast>(
        &mut self,
        fn_name: &str,
        sig: &'ast syn::Signature,
        body: &'ast syn::Block,
    ) {
        let self_type = self.impl_type_stack.last().cloned();
        let canonical = canonical_fn_name(self.path, self_type.as_deref(), fn_name);
        let ctx = FnContext {
            body,
            signature_params: extract_signature_params(sig),
            self_type,
            alias_map: self.alias_map,
            importing_file: self.path,
        };
        let calls = collect_canonical_calls(&ctx);
        self.graph.add_node(&canonical, self.path);
        for callee in calls {
            self.graph.add_edge(&canonical, &callee);
        }
    }
}

impl<'a, 'ast> Visit<'ast> for FileFnCollector<'a> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        self.record_fn(&name, &node.sig, &node.block);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let segs = impl_self_ty_segments(&node.self_ty).unwrap_or_default();
        self.impl_type_stack.push(segs);
        syn::visit::visit_item_impl(self, node);
        self.impl_type_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        self.record_fn(&name, &node.sig, &node.block);
        syn::visit::visit_impl_item_fn(self, node);
    }
}
