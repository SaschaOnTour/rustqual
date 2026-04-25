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

use super::bindings::CanonScope;
use super::local_symbols::{collect_local_symbols_scoped, FileScope, LocalSymbols};
use super::pub_fns_visibility::{collect_visible_type_canonicals_workspace, is_visible};
use super::signature_params::extract_signature_params;
use super::workspace_graph::{collect_crate_root_modules, resolve_impl_self_type};
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use crate::adapters::shared::use_tree::gather_alias_map_scoped;
use crate::adapters::shared::use_tree::ScopedAliasMap;
use std::collections::{HashMap, HashSet};
use syn::visit::Visit;

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
    transparent_wrappers: &HashSet<String>,
) -> HashMap<String, Vec<PubFnInfo<'ast>>> {
    let crate_root_modules = collect_crate_root_modules(files);
    let visible_canonicals = collect_visible_type_canonicals_workspace(
        files,
        cfg_test_files,
        aliases_per_file,
        &crate_root_modules,
        transparent_wrappers,
    );
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
        let LocalSymbols { flat, by_name } = collect_local_symbols_scoped(ast);
        let aliases_per_scope = gather_alias_map_scoped(ast);
        let file = FileScope {
            path,
            alias_map,
            aliases_per_scope: &aliases_per_scope,
            local_symbols: &flat,
            local_decl_scopes: &by_name,
            crate_root_modules: &crate_root_modules,
        };
        let mut collector = PubFnCollector {
            file_path: path.to_string(),
            file: &file,
            found: Vec::new(),
            visible_canonicals: &visible_canonicals,
            impl_stack: Vec::new(),
            mod_stack: Vec::new(),
            enclosing_mod_visible: true,
        };
        collector.visit_file(ast);
        out.entry(layer).or_default().extend(collector.found);
    }
    out
}

/// Workspace-walker — visits items, tracks impl-type visibility
/// for nested impl methods, collects pub fn metadata.
struct PubFnCollector<'ast, 'vis> {
    /// Owning copy of the file path — kept on the collector because
    /// `PubFnInfo` is constructed for each fn, each takes the file
    /// path by value, and `file.path: &str` from the borrowed
    /// `FileScope` doesn't satisfy `String` ownership requirements.
    file_path: String,
    file: &'vis FileScope<'vis>,
    found: Vec<PubFnInfo<'ast>>,
    /// Workspace-wide set of canonical paths of publicly named types.
    /// `crate::<file_modules>::<mod_stack>::<ident>` joined as one
    /// string, comparable directly against `resolve_impl_self_type`'s
    /// output. Shared across files.
    visible_canonicals: &'vis HashSet<String>,
    /// Stack of enclosing `impl` blocks: `(self-type segments, is-visible)`.
    impl_stack: Vec<(Vec<String>, bool)>,
    /// Names of enclosing inline `mod inner { ... }` blocks.
    mod_stack: Vec<String>,
    /// True when every enclosing inline `mod` carries a visibility
    /// modifier. False as soon as any ancestor is private. Top-level
    /// items are always visible. Without this, `mod private { pub fn
    /// helper() {} }` would record `helper` even though it's not
    /// reachable from outside the parent module.
    enclosing_mod_visible: bool,
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
            file: self.file_path.clone(),
            fn_name: name,
            line,
            body,
            signature_params: extract_signature_params(sig),
            self_type: self.current_self_type(),
            mod_stack: self.mod_stack.clone(),
        });
    }
}

/// True iff the `#[test]` / `#[cfg(test)]` attribute set would make
/// this fn a test-harness item (excluded from the check).
fn is_test_fn(attrs: &[syn::Attribute]) -> bool {
    has_test_attr(attrs) || has_cfg_test(attrs)
}

impl<'ast, 'vis> Visit<'ast> for PubFnCollector<'ast, 'vis> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if self.enclosing_mod_visible && is_visible(&node.vis) && !is_test_fn(&node.attrs) {
            let line = syn::spanned::Spanned::span(&node.sig.ident).start().line;
            let name = node.sig.ident.to_string();
            self.record_fn(name, line, &node.block, &node.sig);
        }
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        // Resolve the impl's self-type through the same canonicalisation
        // pipeline used by receiver-tracked method calls, then probe
        // the workspace `visible_canonicals` set with the joined path.
        // Canonical comparison handles short-name collisions
        // (`api::Session` vs `internal::Session`), private-mod impls
        // for top-level pub types (`mod methods { impl super::Session
        // … }`), and re-exports (`pub use private::Hidden`) uniformly.
        // Unresolved self-types (trait objects, references) bring an
        // empty segment list with `visible=false` and the methods
        // are skipped regardless.
        let canonical_segs = resolve_impl_self_type(
            &node.self_ty,
            &CanonScope {
                file: self.file,
                mod_stack: &self.mod_stack,
            },
        )
        .unwrap_or_default();
        let visible = !canonical_segs.is_empty()
            && self.visible_canonicals.contains(&canonical_segs.join("::"));
        self.impl_stack.push((canonical_segs, visible));
        syn::visit::visit_item_impl(self, node);
        self.impl_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        // No enclosing-mod-visible gate here: `visible_canonicals`
        // already encodes whether the type is reachable, so impls in
        // private modules for publicly named types record correctly
        // and impls on private types are filtered uniformly.
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
        let parent_visible = self.enclosing_mod_visible;
        self.enclosing_mod_visible = parent_visible && is_visible(&node.vis);
        self.mod_stack.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.mod_stack.pop();
        self.enclosing_mod_visible = parent_visible;
    }
}
