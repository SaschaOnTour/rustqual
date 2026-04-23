//! Pub-fn enumeration grouped by architecture layer.
//!
//! For both Check A (adapter-must-delegate) and Check B (parity-coverage)
//! we need every `pub fn` in every configured layer. Private fns (no
//! visibility modifier) are helpers and not part of the architectural
//! surface; `pub(crate)` / `pub(super)` / `pub(in path)` are treated as
//! "visible enough" because workspace-internal crates commonly expose
//! their surface through these narrower visibilities.
//!
//! Excluded up-front:
//! - Files flagged as cfg-test by `collect_cfg_test_file_paths`
//!   (those are test harness code, not architectural surface).
//! - Fns carrying `#[test]` / `#[cfg(test)]` attributes (even if pub).
//! - Impl methods whose enclosing `impl Type { ... }` is for a private
//!   (no-modifier) type — the method is unreachable from outside the
//!   file.
//!
//! See Task 2 in the v1.1.0 plan for the full test list.

use super::workspace_graph::{extract_signature_params, impl_self_ty_segments};
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use std::collections::{HashMap, HashSet};
use syn::visit::Visit;
use syn::Visibility;

/// Shape used by both Check A and Check B — we need the fn name to
/// build the canonical-call-target string, the body to walk, and the
/// source line for the finding anchor.
pub(crate) struct PubFnInfo<'ast> {
    pub file: String,
    pub fn_name: String,
    pub line: usize,
    pub body: &'ast syn::Block,
    /// Signature parameters, parallel to `FnContext.signature_params`.
    pub signature_params: Vec<(String, &'ast syn::Type)>,
    /// Type-name path of the enclosing `impl`, if any. Forms the
    /// `Self::method` resolution context for the call collector.
    pub self_type: Option<Vec<String>>,
}

// qual:api
/// Group every `pub` / `pub(crate)` / `pub(super)` / `pub(in path)` fn
/// by the layer of its source file. Test-attribute fns, files in
/// `cfg_test_files`, and impl methods on private types are skipped.
/// Integration: delegates per-file layer lookup + per-file collection.
pub(crate) fn collect_pub_fns_by_layer<'ast>(
    files: &'ast [(String, String, &'ast syn::File)],
    layers: &LayerDefinitions,
    cfg_test_files: &HashSet<String>,
) -> HashMap<String, Vec<PubFnInfo<'ast>>> {
    let mut out: HashMap<String, Vec<PubFnInfo<'ast>>> = HashMap::new();
    for (path, _src, ast) in files {
        if cfg_test_files.contains(path) {
            continue;
        }
        let Some(layer) = layers.layer_for_file(path) else {
            continue;
        };
        let layer = layer.to_string();
        let visible_types = collect_visible_type_names(ast);
        let mut collector = PubFnCollector {
            file: path.clone(),
            found: Vec::new(),
            visible_types,
            impl_stack: Vec::new(),
        };
        collector.visit_file(ast);
        out.entry(layer).or_default().extend(collector.found);
    }
    out
}

/// Pre-pass: collect the names of top-level struct / enum / union /
/// trait / type-alias declarations whose visibility modifier makes them
/// reachable (anything but `Visibility::Inherited`). Impl methods on
/// types not in this set are skipped.
fn collect_visible_type_names(ast: &syn::File) -> HashSet<String> {
    ast.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Struct(s) if is_visible(&s.vis) => Some(s.ident.to_string()),
            syn::Item::Enum(e) if is_visible(&e.vis) => Some(e.ident.to_string()),
            syn::Item::Union(u) if is_visible(&u.vis) => Some(u.ident.to_string()),
            syn::Item::Trait(t) if is_visible(&t.vis) => Some(t.ident.to_string()),
            syn::Item::Type(t) if is_visible(&t.vis) => Some(t.ident.to_string()),
            _ => None,
        })
        .collect()
}

/// Workspace-walker — visits items, tracks impl-type visibility
/// for nested impl methods, collects pub fn metadata.
struct PubFnCollector<'ast> {
    file: String,
    found: Vec<PubFnInfo<'ast>>,
    /// Pre-computed set of type names in this file whose declaration
    /// carries a visibility modifier (pub / pub(crate) / pub(super) /
    /// pub(in path)). Impl methods on any type not in this set are
    /// skipped — impls on private types aren't reachable from outside.
    visible_types: HashSet<String>,
    /// Stack of enclosing `impl` blocks: `(self-type segments, is-visible)`.
    /// Merged so the two halves can't drift out of sync.
    impl_stack: Vec<(Vec<String>, bool)>,
}

impl<'ast> PubFnCollector<'ast> {
    fn current_self_type(&self) -> Option<Vec<String>> {
        self.impl_stack.last().map(|(segs, _)| segs.clone())
    }

    fn current_impl_visible(&self) -> bool {
        self.impl_stack.last().map(|(_, v)| *v).unwrap_or(false)
    }

    fn record_fn(
        &mut self,
        name: String,
        line: usize,
        body: &'ast syn::Block,
        sig: &'ast syn::Signature,
    ) {
        self.found.push(PubFnInfo {
            file: self.file.clone(),
            fn_name: name,
            line,
            body,
            signature_params: extract_signature_params(sig),
            self_type: self.current_self_type(),
        });
    }
}

/// Visibility modifier counts as "visible for the check" iff it's
/// anything other than the implicit (no-modifier) case. See D-5 for
/// the rationale.
fn is_visible(vis: &Visibility) -> bool {
    !matches!(vis, Visibility::Inherited)
}

/// True iff the `#[test]` / `#[cfg(test)]` attribute set would make
/// this fn a test-harness item (excluded from the check).
fn is_test_fn(attrs: &[syn::Attribute]) -> bool {
    has_test_attr(attrs) || has_cfg_test(attrs)
}

impl<'ast> Visit<'ast> for PubFnCollector<'ast> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if is_visible(&node.vis) && !is_test_fn(&node.attrs) {
            let line = syn::spanned::Spanned::span(&node.sig.ident).start().line;
            let name = node.sig.ident.to_string();
            self.record_fn(name, line, &node.block, &node.sig);
        }
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        // Conservative: an impl whose self-type we can't parse (trait
        // objects, generics) gets empty segs + invisible — its methods
        // fall out of the check, matching "can't resolve, don't count".
        let self_segs = impl_self_ty_segments(&node.self_ty);
        let visible = self_segs
            .as_ref()
            .and_then(|segs| segs.last())
            .is_some_and(|name| self.visible_types.contains(name));
        self.impl_stack
            .push((self_segs.unwrap_or_default(), visible));
        syn::visit::visit_item_impl(self, node);
        self.impl_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if self.current_impl_visible() && is_visible(&node.vis) && !is_test_fn(&node.attrs) {
            let line = syn::spanned::Spanned::span(&node.sig.ident).start().line;
            let name = node.sig.ident.to_string();
            self.record_fn(name, line, &node.block, &node.sig);
        }
        syn::visit::visit_impl_item_fn(self, node);
    }
}
