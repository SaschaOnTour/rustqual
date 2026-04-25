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

use super::signature_params::extract_signature_params;
use super::workspace_graph::{
    collect_crate_root_modules, collect_local_symbols, impl_self_ty_segments,
    resolve_impl_self_type,
};
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
    /// Names of enclosing inline `mod inner { ... }` blocks, outer-most
    /// first. Feeds the canonical-name builder so nested-mod items key
    /// under `crate::<file>::inner::…` to match the graph + type index.
    pub mod_stack: Vec<String>,
}

// qual:api
/// Group every `pub` / `pub(crate)` / `pub(super)` / `pub(in path)` fn
/// by the layer of its source file. Test-attribute fns, files in
/// `cfg_test_files`, and impl methods on private types are skipped.
/// Integration: delegates per-file layer lookup + per-file collection.
pub(crate) fn collect_pub_fns_by_layer<'ast>(
    files: &[(&'ast str, &'ast syn::File)],
    aliases_per_file: &HashMap<String, HashMap<String, Vec<String>>>,
    layers: &LayerDefinitions,
    cfg_test_files: &HashSet<String>,
) -> HashMap<String, Vec<PubFnInfo<'ast>>> {
    let visible_types = collect_visible_type_names_workspace(files, cfg_test_files);
    let crate_root_modules = collect_crate_root_modules(files);
    let empty_aliases = HashMap::new();
    let mut out: HashMap<String, Vec<PubFnInfo<'ast>>> = HashMap::new();
    for (path, ast) in files {
        if cfg_test_files.contains(*path) {
            continue;
        }
        let Some(layer) = layers.layer_for_file(path) else {
            continue;
        };
        let layer = layer.to_string();
        // Share the call-parity entrypoint's `aliases_per_file` map so
        // we don't re-walk the UseTree per file (each walk is a full
        // `gather_alias_map`). Fall back to an empty map for files not
        // in the pre-computed set — those files won't have resolvable
        // impl self-types via `use` anyway, and the local-symbol /
        // crate-root fallbacks still work.
        let alias_map = aliases_per_file.get(*path).unwrap_or(&empty_aliases);
        let local_symbols = collect_local_symbols(ast);
        let mut collector = PubFnCollector {
            file: path.to_string(),
            found: Vec::new(),
            visible_types: &visible_types,
            alias_map,
            local_symbols: &local_symbols,
            crate_root_modules: &crate_root_modules,
            impl_stack: Vec::new(),
            mod_stack: Vec::new(),
        };
        collector.visit_file(ast);
        out.entry(layer).or_default().extend(collector.found);
    }
    out
}

/// Collect every visible (non-inherited-visibility) top-level type name
/// across the whole non-test workspace. Impls on the same type name get
/// counted as visible regardless of which file the impl lives in — so
/// `pub struct Session` in `src/app/session.rs` and its `impl Session`
/// in a companion file both contribute to the check.
///
/// The matching is string-equality on the last segment of the impl's
/// self-type path. Two distinct types with the same name in different
/// files both match; that's MVP-level imprecision — false positives
/// (over-counting) rather than false negatives.
fn collect_visible_type_names_workspace(
    files: &[(&str, &syn::File)],
    cfg_test_files: &HashSet<String>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    for (path, ast) in files {
        if cfg_test_files.contains(*path) {
            continue;
        }
        for item in &ast.items {
            match item {
                syn::Item::Struct(s) if is_visible(&s.vis) => {
                    out.insert(s.ident.to_string());
                }
                syn::Item::Enum(e) if is_visible(&e.vis) => {
                    out.insert(e.ident.to_string());
                }
                syn::Item::Union(u) if is_visible(&u.vis) => {
                    out.insert(u.ident.to_string());
                }
                syn::Item::Trait(t) if is_visible(&t.vis) => {
                    out.insert(t.ident.to_string());
                }
                syn::Item::Type(t) if is_visible(&t.vis) => {
                    out.insert(t.ident.to_string());
                }
                _ => {}
            }
        }
    }
    out
}

/// Workspace-walker — visits items, tracks impl-type visibility
/// for nested impl methods, collects pub fn metadata.
struct PubFnCollector<'ast, 'vis> {
    file: String,
    found: Vec<PubFnInfo<'ast>>,
    /// Workspace-wide set of type names whose declaration carries a
    /// visibility modifier. Impls on any type not in this set are
    /// skipped — impls on private types aren't reachable from outside
    /// their declaring file. Shared across files so cross-file impls
    /// on a `pub struct` are correctly recognised.
    visible_types: &'vis HashSet<String>,
    /// File's import aliases — used to canonicalise impl self-types so
    /// `use crate::app::Session; impl Session { ... }` and the call
    /// collector's receiver-tracked canonical agree on the same path.
    alias_map: &'vis HashMap<String, Vec<String>>,
    /// Same-file top-level item names for the local-symbol fallback.
    local_symbols: &'vis HashSet<String>,
    /// Workspace crate-root module names for Rust 2018+ absolute imports.
    crate_root_modules: &'vis HashSet<String>,
    /// Stack of enclosing `impl` blocks: `(self-type segments, is-visible)`.
    /// Merged so the two halves can't drift out of sync.
    impl_stack: Vec<(Vec<String>, bool)>,
    /// Names of enclosing inline `mod inner { ... }` blocks. Feeds
    /// `PubFnInfo.mod_stack` so Check B canonical names align with the
    /// graph keys / type index (both of which now prefix inline mods).
    mod_stack: Vec<String>,
}

impl<'ast, 'vis> PubFnCollector<'ast, 'vis> {
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
            mod_stack: self.mod_stack.clone(),
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

impl<'ast, 'vis> Visit<'ast> for PubFnCollector<'ast, 'vis> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if is_visible(&node.vis) && !is_test_fn(&node.attrs) {
            let line = syn::spanned::Spanned::span(&node.sig.ident).start().line;
            let name = node.sig.ident.to_string();
            self.record_fn(name, line, &node.block, &node.sig);
        }
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        // Visibility is still name-based (last segment of the raw impl
        // header), but the self-type we store runs through the same
        // canonicalisation pipeline as receiver-tracked method calls
        // — otherwise `use crate::app::Session; impl Session { ... }`
        // would produce `crate::<file_mod>::Session::method` for
        // Check B while the call collector resolves caller sites to
        // `crate::app::Session::method`, and every method on an
        // imported type would be silently "unreached".
        let raw_segs = impl_self_ty_segments(&node.self_ty);
        let visible = raw_segs
            .as_ref()
            .and_then(|segs| segs.last())
            .is_some_and(|name| self.visible_types.contains(name));
        // `visible` already gates on the raw-segments path (last segment
        // in the workspace-wide visible-types set), so unresolved
        // self-types (trait objects, references) bring `visible=false`
        // with them and the method is skipped regardless. Coalescing
        // `None` to an empty segment list here is safe because it's
        // never read under that flag.
        let canonical_segs = resolve_impl_self_type(
            &node.self_ty,
            self.alias_map,
            self.local_symbols,
            self.crate_root_modules,
            &self.file,
        )
        .unwrap_or_default();
        self.impl_stack.push((canonical_segs, visible));
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

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        // Skip inline `#[cfg(test)] mod tests { ... }` blocks so test
        // helpers can't leak into the pub-fn surface and produce
        // spurious call_parity findings.
        if has_cfg_test(&node.attrs) {
            return;
        }
        self.mod_stack.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.mod_stack.pop();
    }
}
