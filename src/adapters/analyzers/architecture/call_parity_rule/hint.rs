//! Discoverable hints for `CallParityMissingAdapter` findings —
//! point the author at private + non-stdlib-attributed fns in the
//! missing adapter that would resolve the finding if their attribute
//! were promoted via `[architecture.call_parity] promoted_attributes`.
//! The candidate predicate composes four filters; see
//! `compute_hint_for_target` for the precise definition.

use super::pub_fns::PubFnInfo;
use super::workspace_graph::{canonical_fn_name, canonical_name_for_pub_fn, CallGraph};
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use std::collections::{HashMap, HashSet, VecDeque};
use syn::visit::Visit;

/// Attribute names that are part of the stdlib / cargo ecosystem and
/// don't mark framework-handler intent. A private fn carrying *only*
/// these is excluded from hint candidacy.
const STDLIB_ATTRS: &[&str] = &[
    "allow",
    "deny",
    "warn",
    "forbid",
    "deprecated",
    "inline",
    "cold",
    "must_use",
    "doc",
    "cfg",
    "cfg_attr",
    "test",
    "derive",
    "repr",
    "non_exhaustive",
    "no_mangle",
    "link",
    "automatically_derived",
    "track_caller",
    "expect",
];

/// One private + attributed fn that survives the stdlib-attribute
/// filter. Carries the source location for hint rendering and the
/// attribute names so the hint can name them explicitly.
pub(crate) struct PrivateCandidate {
    pub canonical: String,
    pub file: String,
    pub line: usize,
    pub fn_name: String,
    pub layer: Option<String>,
    pub attr_names: Vec<String>,
}

// qual:api
/// Walk every workspace file once, return every private fn that
/// carries at least one non-stdlib attribute. `pub_fns_by_layer` is
/// consulted to skip fns that already qualify as public — those are
/// not private and won't generate a candidate. Operation: per-file
/// AST traversal + filtering.
pub(super) fn collect_private_candidates(
    files: &[(&str, &syn::File)],
    pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'_>>>,
    layers: &LayerDefinitions,
) -> Vec<PrivateCandidate> {
    let pub_canonicals = pub_fn_canonicals(pub_fns_by_layer);
    let mut out = Vec::new();
    for (path, ast) in files {
        let mut collector = CandidateCollector {
            file_path: path.to_string(),
            layer: layers.layer_for_file(path).map(String::from),
            pub_canonicals: &pub_canonicals,
            mod_stack: Vec::new(),
            impl_stack: Vec::new(),
            found: &mut out,
        };
        collector.visit_file(ast);
    }
    out
}

// qual:api
/// Attach a hint to every `CallParityMissingAdapter` finding for
/// which a private attributed candidate would resolve the gap.
/// Empty `candidates` short-circuits — the common case when no
/// promotable attribute exists in the workspace.
pub(super) fn enrich_with_hints(
    findings: &mut [MatchLocation],
    graph: &CallGraph,
    candidates: &[PrivateCandidate],
) {
    if candidates.is_empty() {
        return;
    }
    let by_adapter = group_by_adapter(candidates);
    let mut probe = HintProbe {
        graph,
        by_adapter: &by_adapter,
        upstream_cache: HashMap::new(),
    };
    for f in findings {
        if let ViolationKind::CallParityMissingAdapter {
            target_fn,
            missing_adapters,
            hint,
            ..
        } = &mut f.kind
        {
            *hint = probe.hint_for(target_fn, missing_adapters);
        }
    }
}

/// Per-call probe state. The `upstream_cache` memoises one reverse
/// BFS per unique `target_fn` so multiple findings on the same
/// target share a single graph walk.
struct HintProbe<'a> {
    graph: &'a CallGraph,
    by_adapter: &'a HashMap<&'a str, Vec<&'a PrivateCandidate>>,
    upstream_cache: HashMap<String, HashSet<String>>,
}

impl<'a> HintProbe<'a> {
    fn hint_for(&mut self, target_fn: &str, missing_adapters: &[String]) -> Option<String> {
        let graph = self.graph;
        let upstream = self
            .upstream_cache
            .entry(target_fn.to_string())
            .or_insert_with(|| reverse_bfs(graph, target_fn));
        let by_adapter = collect_hits_per_adapter(self.by_adapter, missing_adapters, upstream);
        if by_adapter.is_empty() {
            None
        } else {
            Some(format_hint(&by_adapter))
        }
    }
}

/// Reverse BFS from `target` over `graph.reverse` — every node from
/// which `target` is transitively reachable. Excludes `target`
/// itself; candidates can never coincide with the target since they
/// come from the missing-adapter layer, not the target layer.
fn reverse_bfs(graph: &CallGraph, target: &str) -> HashSet<String> {
    let mut upstream: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    enqueue_callers(graph, target, &mut upstream, &mut queue);
    while let Some(node) = queue.pop_front() {
        enqueue_callers(graph, &node, &mut upstream, &mut queue);
    }
    upstream
}

fn enqueue_callers(
    graph: &CallGraph,
    node: &str,
    upstream: &mut HashSet<String>,
    queue: &mut VecDeque<String>,
) {
    let Some(callers) = graph.reverse.get(node) else {
        return;
    };
    for c in callers {
        if upstream.insert(c.clone()) {
            queue.push_back(c.clone());
        }
    }
}

/// Group candidates by their layer name (skipping layerless files).
/// One pass over the input; downstream lookups become O(1) per
/// (finding, missing_adapter) pair.
fn group_by_adapter(candidates: &[PrivateCandidate]) -> HashMap<&str, Vec<&PrivateCandidate>> {
    let mut out: HashMap<&str, Vec<&PrivateCandidate>> = HashMap::new();
    for c in candidates {
        if let Some(layer) = c.layer.as_deref() {
            out.entry(layer).or_default().push(c);
        }
    }
    out
}

/// Per missing adapter, intersect its candidates with `upstream` and
/// keep adapters that have at least one hit. Sorted (file, line,
/// fn_name) for deterministic hint output.
fn collect_hits_per_adapter<'a>(
    by_adapter: &'a HashMap<&'a str, Vec<&'a PrivateCandidate>>,
    missing_adapters: &[String],
    upstream: &HashSet<String>,
) -> Vec<(String, Vec<&'a PrivateCandidate>)> {
    let mut out: Vec<(String, Vec<&PrivateCandidate>)> = Vec::new();
    for adapter in missing_adapters {
        let Some(adapter_candidates) = by_adapter.get(adapter.as_str()) else {
            continue;
        };
        let mut hits: Vec<&PrivateCandidate> = adapter_candidates
            .iter()
            .copied()
            .filter(|c| upstream.contains(&c.canonical))
            .collect();
        if !hits.is_empty() {
            hits.sort_by(|a, b| {
                a.file
                    .cmp(&b.file)
                    .then(a.line.cmp(&b.line))
                    .then(a.fn_name.cmp(&b.fn_name))
            });
            out.push((adapter.clone(), hits));
        }
    }
    out
}

/// Project `pub_fns_by_layer` into the set of canonical names already
/// considered public. Used to skip those fns during the private
/// candidate walk. Operation: nested-collect projection.
fn pub_fn_canonicals(pub_fns_by_layer: &HashMap<String, Vec<PubFnInfo<'_>>>) -> HashSet<String> {
    pub_fns_by_layer
        .values()
        .flat_map(|infos| infos.iter().map(canonical_name_for_pub_fn))
        .collect()
}

/// True iff `attrs` contains at least one attribute whose leaf-ident
/// is not in `STDLIB_ATTRS`. Operation: per-attribute leaf probe.
fn has_non_stdlib_attribute(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .is_some_and(|name| !STDLIB_ATTRS.contains(&name.as_str()))
    })
}

/// Collect the non-stdlib attribute names from `attrs`. Operation:
/// per-attribute leaf projection.
fn non_stdlib_attribute_names(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|a| {
            a.path()
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .filter(|name| !STDLIB_ATTRS.contains(&name.as_str()))
        })
        .collect()
}

/// Render the final hint string. Operation: per-adapter block
/// assembly.
fn format_hint(by_adapter: &[(String, Vec<&PrivateCandidate>)]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for (adapter, hits) in by_adapter {
        let (noun, verb) = if hits.len() == 1 {
            ("method", "reaches")
        } else {
            ("methods", "reach")
        };
        lines.push(format!(
            "{} private {noun} in {adapter} transitively {verb} this target:",
            hits.len()
        ));
        for c in hits {
            let attrs = c
                .attr_names
                .iter()
                .map(|n| format!("#[{n}]"))
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(format!(
                "  - {}:{} {} has {} attribute(s)",
                c.file, c.line, c.fn_name, attrs
            ));
        }
    }
    lines.push(
        "Add the attribute name to `[architecture.call_parity] promoted_attributes` if it marks a macro-generated handler entry point."
            .to_string(),
    );
    lines.join("\n")
}

/// AST walker that records every private + non-stdlib-attributed fn,
/// tracking the enclosing impl-self-type and inline-mod stack so the
/// canonical name matches the workspace call graph.
struct CandidateCollector<'a> {
    file_path: String,
    layer: Option<String>,
    pub_canonicals: &'a HashSet<String>,
    mod_stack: Vec<String>,
    impl_stack: Vec<Option<Vec<String>>>,
    found: &'a mut Vec<PrivateCandidate>,
}

impl<'a> CandidateCollector<'a> {
    fn record_if_candidate(&mut self, sig: &syn::Signature, attrs: &[syn::Attribute]) {
        if has_cfg_test(attrs) || has_test_attr(attrs) {
            return;
        }
        if !has_non_stdlib_attribute(attrs) {
            return;
        }
        let fn_name = sig.ident.to_string();
        let self_type = match self.impl_stack.last() {
            None => None,
            Some(Some(segs)) => Some(segs.as_slice()),
            Some(None) => return,
        };
        let canonical = canonical_fn_name(&self.file_path, self_type, &self.mod_stack, &fn_name);
        // Skip fns that already count as public — those are handlers,
        // not candidates for promotion.
        if self.pub_canonicals.contains(&canonical) {
            return;
        }
        let line = syn::spanned::Spanned::span(&sig.ident).start().line;
        self.found.push(PrivateCandidate {
            canonical,
            file: self.file_path.clone(),
            line,
            fn_name,
            layer: self.layer.clone(),
            attr_names: non_stdlib_attribute_names(attrs),
        });
    }
}

impl<'ast, 'a> Visit<'ast> for CandidateCollector<'a> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.record_if_candidate(&node.sig, &node.attrs);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        let resolved = match node.self_ty.as_ref() {
            syn::Type::Path(p) => Some(
                p.path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        };
        self.impl_stack.push(resolved);
        syn::visit::visit_item_impl(self, node);
        self.impl_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.record_if_candidate(&node.sig, &node.attrs);
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        self.mod_stack.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.mod_stack.pop();
    }
}
