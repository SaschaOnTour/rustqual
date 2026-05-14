//! Per-file walk that collects syntactically private fns carrying a
//! non-stdlib attribute — the candidate set the probe in `mod.rs`
//! intersects with each missing-adapter's reverse-reachable nodes.

use super::super::bindings::CanonScope;
use super::super::local_symbols::{collect_local_symbols_scoped, FileScope, LocalSymbols};
use super::super::workspace_graph::{
    canonical_fn_name, collect_crate_root_modules, resolve_impl_self_type,
};
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use crate::adapters::shared::use_tree::{gather_alias_map, gather_alias_map_scoped};
use std::collections::HashSet;
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
/// Walk every workspace file once, return every syntactically private
/// fn that carries at least one non-stdlib attribute. Files in
/// `cfg_test_files` are skipped — their fns disappear from the call
/// graph so a hint pointing at them would never resolve. Operation:
/// per-file scope build + AST traversal + filtering.
pub(crate) fn collect_private_candidates(
    files: &[(&str, &syn::File)],
    cfg_test_files: &HashSet<String>,
    layers: &LayerDefinitions,
) -> Vec<PrivateCandidate> {
    let crate_root_modules = collect_crate_root_modules(files);
    let mut out = Vec::new();
    for (path, ast) in files {
        if cfg_test_files.contains(*path) {
            continue;
        }
        let alias_map = gather_alias_map(ast);
        let aliases_per_scope = gather_alias_map_scoped(ast);
        let LocalSymbols { flat, by_name } = collect_local_symbols_scoped(ast);
        let file = FileScope {
            path,
            alias_map: &alias_map,
            aliases_per_scope: &aliases_per_scope,
            local_symbols: &flat,
            local_decl_scopes: &by_name,
            crate_root_modules: &crate_root_modules,
        };
        let mut collector = CandidateCollector {
            file: &file,
            layer: layers.layer_for_file(path).map(String::from),
            mod_stack: Vec::new(),
            impl_stack: Vec::new(),
            found: &mut out,
        };
        collector.visit_file(ast);
    }
    out
}

/// AST walker that records every syntactically private +
/// non-stdlib-attributed fn, tracking the enclosing impl-self-type
/// (canonicalised via the same `resolve_impl_self_type` pipeline as
/// `pub_fns` and `file_fn_collector` so reverse-BFS lookups against
/// the workspace graph hit) and inline-mod stack.
struct CandidateCollector<'a, 'vis> {
    file: &'vis FileScope<'vis>,
    layer: Option<String>,
    mod_stack: Vec<String>,
    impl_stack: Vec<Option<Vec<String>>>,
    found: &'a mut Vec<PrivateCandidate>,
}

impl<'a, 'vis> CandidateCollector<'a, 'vis> {
    /// Promotion only lifts the visibility check in `pub_fns.rs` — fns
    /// excluded for other reasons (private mod chain, private impl
    /// self-type) stay invisible regardless. Restricting candidates to
    /// `Visibility::Inherited` rules out hints that tell the author to
    /// add an attribute that wouldn't actually fix the finding.
    fn record_if_candidate(
        &mut self,
        sig: &syn::Signature,
        vis: &syn::Visibility,
        attrs: &[syn::Attribute],
    ) {
        if !matches!(vis, syn::Visibility::Inherited) {
            return;
        }
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
        let canonical = canonical_fn_name(self.file.path, self_type, &self.mod_stack, &fn_name);
        let line = syn::spanned::Spanned::span(&sig.ident).start().line;
        self.found.push(PrivateCandidate {
            canonical,
            file: self.file.path.to_string(),
            line,
            fn_name,
            layer: self.layer.clone(),
            attr_names: non_stdlib_attribute_names(attrs),
        });
    }
}

impl<'ast, 'a, 'vis> Visit<'ast> for CandidateCollector<'a, 'vis> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.record_if_candidate(&node.sig, &node.vis, &node.attrs);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        let scope = CanonScope {
            file: self.file,
            mod_stack: &self.mod_stack,
        };
        let resolved = resolve_impl_self_type(&node.self_ty, &scope);
        self.impl_stack.push(resolved);
        syn::visit::visit_item_impl(self, node);
        self.impl_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        self.record_if_candidate(&node.sig, &node.vis, &node.attrs);
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
