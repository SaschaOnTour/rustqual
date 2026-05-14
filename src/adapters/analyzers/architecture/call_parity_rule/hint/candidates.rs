//! Per-file walk that collects syntactically private fns carrying a
//! non-stdlib attribute — the candidate set the probe in `mod.rs`
//! intersects with each missing-adapter's reverse-reachable nodes.

use super::super::bindings::CanonScope;
use super::super::file_visibility::collect_file_root_visibility;
use super::super::local_symbols::{collect_local_symbols_scoped, FileScope, LocalSymbols};
use super::super::pub_fns_visibility::{collect_visible_type_canonicals_workspace, is_visible};
use super::super::workspace_graph::{
    canonical_fn_name, collect_crate_root_modules, resolve_impl_self_type,
};
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use crate::adapters::shared::use_tree::{gather_alias_map, gather_alias_map_scoped};
use std::collections::{HashMap, HashSet};
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
/// fn that carries at least one non-stdlib attribute AND lives on a
/// would-be-visible surface (visible enclosing mod chain, visible
/// impl self-type, file reachable via `pub mod` from crate root).
/// Files in `cfg_test_files` are skipped — their fns disappear from
/// the call graph so a hint pointing at them would never resolve.
/// Restricting candidates this strictly mirrors what `pub_fns.rs`
/// would record AFTER promotion: a hint that names a fn here
/// guarantees that adding the attribute to `promoted_attributes`
/// puts it on the handler surface, no other visibility layer blocks.
/// Operation: per-file scope build + AST traversal + filtering.
pub(crate) fn collect_private_candidates(
    files: &[(&str, &syn::File)],
    cfg_test_files: &HashSet<String>,
    aliases_per_file: &HashMap<String, HashMap<String, Vec<String>>>,
    layers: &LayerDefinitions,
    transparent_wrappers: &HashSet<String>,
) -> Vec<PrivateCandidate> {
    let crate_root_modules = collect_crate_root_modules(files);
    let file_root_visibility = collect_file_root_visibility(files);
    let visible_canonicals = collect_visible_type_canonicals_workspace(
        files,
        cfg_test_files,
        aliases_per_file,
        &crate_root_modules,
        transparent_wrappers,
    );
    let empty_aliases = HashMap::new();
    let mut out = Vec::new();
    for (path, ast) in files {
        if cfg_test_files.contains(*path) {
            continue;
        }
        let alias_map = aliases_per_file.get(*path).unwrap_or(&empty_aliases);
        let aliases_per_scope = gather_alias_map_scoped(ast);
        let LocalSymbols { flat, by_name } = collect_local_symbols_scoped(ast);
        let file = FileScope {
            path,
            alias_map,
            aliases_per_scope: &aliases_per_scope,
            local_symbols: &flat,
            local_decl_scopes: &by_name,
            crate_root_modules: &crate_root_modules,
        };
        let file_visible = file_root_visibility.get(*path).copied().unwrap_or(true);
        let mut collector = CandidateCollector {
            file: &file,
            layer: layers.layer_for_file(path).map(String::from),
            visible_canonicals: &visible_canonicals,
            mod_stack: Vec::new(),
            impl_stack: Vec::new(),
            enclosing_mod_visible: file_visible,
            found: &mut out,
        };
        collector.visit_file(ast);
    }
    out
}

/// AST walker that records every syntactically private +
/// non-stdlib-attributed fn whose surrounding visibility chain
/// (file-root mod, enclosing inline mods, impl self-type) wouldn't
/// block the fn from `pub_fns_by_layer` after promotion. Mirrors
/// `pub_fns::PubFnCollector`'s tracking so the candidate predicate
/// matches "would pub_fns record this if `promoted_attributes`
/// matched the attribute".
struct CandidateCollector<'a, 'vis> {
    file: &'vis FileScope<'vis>,
    layer: Option<String>,
    visible_canonicals: &'vis HashSet<String>,
    mod_stack: Vec<String>,
    /// `(self_type_segs, self_type_visible)` per enclosing impl,
    /// matching the first two slots of `pub_fns::impl_stack`.
    impl_stack: Vec<(Vec<String>, bool)>,
    /// True iff every ancestor inline `mod` is visibility-modified
    /// AND the file's parent `mod foo;` declaration is `pub`. Mirror
    /// of `pub_fns::PubFnCollector::enclosing_mod_visible`.
    enclosing_mod_visible: bool,
    found: &'a mut Vec<PrivateCandidate>,
}

impl<'a, 'vis> CandidateCollector<'a, 'vis> {
    fn current_self_type(&self) -> Option<&[String]> {
        self.impl_stack.last().map(|(segs, _)| segs.as_slice())
    }

    fn current_impl_visible(&self) -> bool {
        self.impl_stack.last().map(|(_, v)| *v).unwrap_or(true)
    }

    /// Promotion only flips the syntactic-visibility check on the fn
    /// itself — every other visibility layer (enclosing mod chain,
    /// impl self-type visibility, file-root visibility) is unchanged.
    /// Skipping candidates where any of those would still block is
    /// the difference between an actionable hint and a misleading
    /// one.
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
        if !self.enclosing_mod_visible {
            return;
        }
        if !self.impl_stack.is_empty() && !self.current_impl_visible() {
            return;
        }
        let fn_name = sig.ident.to_string();
        let self_type = self.current_self_type();
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
        let canonical_segs = resolve_impl_self_type(&node.self_ty, &scope).unwrap_or_default();
        let visible = !canonical_segs.is_empty()
            && self.visible_canonicals.contains(&canonical_segs.join("::"));
        self.impl_stack.push((canonical_segs, visible));
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
        let parent_visible = self.enclosing_mod_visible;
        self.enclosing_mod_visible = parent_visible && is_visible(&node.vis);
        self.mod_stack.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.mod_stack.pop();
        self.enclosing_mod_visible = parent_visible;
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
