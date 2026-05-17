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
use super::file_visibility::collect_file_root_visibility;
use super::local_symbols::{collect_local_symbols_scoped, FileScope, LocalSymbols};
use super::pub_fns_visibility::{collect_visible_type_canonicals_workspace, is_visible};
use super::signature_params::extract_signature_params;
use super::workspace_graph::{collect_crate_root_modules, resolve_impl_self_type};
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use crate::adapters::shared::use_tree::{gather_alias_map_scoped, AliasMap};
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
    /// True iff the fn carries `#[deprecated]` (in any form). Used to
    /// exclude phased-out adapter handlers from call-parity Checks A,
    /// B, C, and D.
    pub deprecated: bool,
}

/// Inputs bundle for `collect_pub_fns_by_layer`. Keeps the public
/// entry point under the SRP-parameter budget while making each
/// per-file lookup table explicit at the call site.
pub(crate) struct PubFnInputs<'a, 'ast> {
    pub files: &'a [(&'ast str, &'ast syn::File)],
    pub aliases_per_file: &'a HashMap<String, AliasMap>,
    pub layers: &'a LayerDefinitions,
    pub transparent_wrappers: &'a HashSet<String>,
    pub promoted_attributes: &'a HashSet<String>,
    /// Workspace-derived metadata pre-computed once per analysis at
    /// the call_parity entry point (`mod.rs::collect_findings`) and
    /// threaded through. Bundles `cfg_test_files` +
    /// `crate_root_modules` + `workspace_module_paths` so the helper
    /// stays under the SRP_PARAMS budget.
    pub workspace: &'a super::local_symbols::WorkspaceLookup<'a>,
}

/// Group every `pub` / `pub(crate)` / `pub(super)` / `pub(in path)` fn
/// by the layer of its source file. Test-attribute fns, files in
/// `cfg_test_files`, and impl methods on private types are skipped.
/// `promoted_attributes` lifts otherwise-private fns onto the
/// adapter-handler surface when they carry a matching attribute —
/// closes the gap for proc-macro-generated dispatch (rmcp `#[tool]`,
/// axum/poem `#[handler]`, etc.).
/// Integration: delegates per-file layer lookup + per-file collection.
pub(crate) fn collect_pub_fns_by_layer<'ast>(
    inputs: PubFnInputs<'_, 'ast>,
) -> HashMap<String, Vec<PubFnInfo<'ast>>> {
    let setup = WorkspaceSetup::build(&inputs);
    let empty_aliases = HashMap::new();
    let mut out: HashMap<String, Vec<PubFnInfo<'ast>>> = HashMap::new();
    for (path, ast) in inputs.files {
        if inputs.workspace.cfg_test_files.contains(*path) {
            continue;
        }
        let Some(layer) = inputs.layers.layer_for_file(path) else {
            continue;
        };
        let layer = layer.to_string();
        // Share the call-parity entrypoint's `aliases_per_file` map so
        // we don't re-walk the UseTree per file (each walk is a full
        // `gather_alias_map`).
        let alias_map = inputs.aliases_per_file.get(*path).unwrap_or(&empty_aliases);
        let LocalSymbols { flat, by_name } = collect_local_symbols_scoped(ast);
        let aliases_per_scope = gather_alias_map_scoped(ast);
        let file = FileScope {
            path,
            alias_map,
            aliases_per_scope: &aliases_per_scope,
            local_symbols: &flat,
            local_decl_scopes: &by_name,
            crate_root_modules: inputs.workspace.crate_root_modules,
            workspace_module_paths: Some(inputs.workspace.workspace_module_paths),
        };
        let file_visible = setup.file_root_visibility.get(*path).copied().unwrap_or(true);
        let mut collector = PubFnCollector {
            file_path: path.to_string(),
            file: &file,
            found: Vec::new(),
            visible_canonicals: &setup.visible_canonicals,
            promoted_attributes: inputs.promoted_attributes,
            reexports: Some(&setup.reexports),
            impl_stack: Vec::new(),
            mod_stack: Vec::new(),
            enclosing_mod_visible: file_visible,
        };
        collector.visit_file(ast);
        out.entry(layer).or_default().extend(collector.found);
    }
    out
}

/// Workspace-wide precomputed lookups bundled together. Built once at
/// the top of `collect_pub_fns_by_layer` so the per-file loop stays
/// under the LONG_FN budget.
struct WorkspaceSetup {
    file_root_visibility: HashMap<String, bool>,
    visible_canonicals: HashSet<String>,
    reexports: super::reexports::ReexportMap,
}

impl WorkspaceSetup {
    fn build(inputs: &PubFnInputs<'_, '_>) -> Self {
        Self {
            file_root_visibility: collect_file_root_visibility(inputs.files),
            visible_canonicals: collect_visible_type_canonicals_workspace(
                inputs.files,
                inputs.aliases_per_file,
                inputs.workspace,
                inputs.transparent_wrappers,
            ),
            reexports: build_reexports_for_pub_fns(
                inputs.files,
                inputs.aliases_per_file,
                inputs.workspace,
            ),
        }
    }
}

/// Build a workspace-wide `pub use` re-export map for the pub-fn
/// collector. Mirrors the build inside `workspace_graph::build_call_graph`
/// so the two passes agree on the DECL canonical for every re-exported
/// type/trait — without this duplicate setup, `pub_fns` and the graph
/// produce different impl-self canonicals on re-exported types and
/// Check B/D fires phantom missing-adapter findings.
///
/// Mild redundancy with the graph builder is accepted as the
/// architecturally correct trade-off: both passes need workspace
/// `FileScope`s + the resulting reexport map, but the build order of
/// `collect_findings` runs `pub_fns` before `build_call_graph`. A
/// future v1.2.6 refactor can hoist this into a shared `WorkspaceSetup`
/// owned by `collect_findings` and threaded into both passes.
fn build_reexports_for_pub_fns(
    files: &[(&str, &syn::File)],
    aliases_per_file: &HashMap<String, AliasMap>,
    workspace: &super::local_symbols::WorkspaceLookup<'_>,
) -> super::reexports::ReexportMap {
    let cfg_test_files = workspace.cfg_test_files;
    let local_symbols_per_file: HashMap<String, LocalSymbols> = files
        .iter()
        .filter(|(p, _)| !cfg_test_files.contains(*p))
        .map(|(path, ast)| (path.to_string(), collect_local_symbols_scoped(ast)))
        .collect();
    let aliases_scoped_per_file: HashMap<
        String,
        crate::adapters::shared::use_tree::ScopedAliasMap,
    > = files
        .iter()
        .filter(|(p, _)| !cfg_test_files.contains(*p))
        .map(|(path, ast)| (path.to_string(), gather_alias_map_scoped(ast)))
        .collect();
    let workspace_files = super::local_symbols::build_workspace_files_map(
        super::local_symbols::WorkspaceFilesInputs {
            files,
            cfg_test_files,
            aliases_per_file,
            aliases_scoped_per_file: &aliases_scoped_per_file,
            local_symbols_per_file: &local_symbols_per_file,
            crate_root_modules: workspace.crate_root_modules,
            workspace_module_paths: Some(workspace.workspace_module_paths),
        },
    );
    super::reexports::collect_reexport_map(files, &workspace_files)
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
    /// Bare attribute names that lift a private fn onto the handler
    /// surface — for proc-macro-generated dispatch where the user
    /// writes `#[tool] async fn search(&self, ...)` (no `pub`) and
    /// the macro generates the public wrapper at expansion time.
    promoted_attributes: &'vis HashSet<String>,
    /// Workspace-wide `pub use` re-export map. `Some(&…)` ensures the
    /// per-impl `CanonScope` constructed in `visit_item_impl` routes
    /// through the same reexport-aware gate as the graph collector,
    /// so impl-self-type canonicals match the graph edges. Without
    /// this, the two sides drift on re-exported types and Check B/D
    /// reports phantom missing-adapter findings.
    reexports: Option<&'vis super::reexports::ReexportMap>,
    /// Stack of enclosing `impl` blocks: `(self-type segments, is-visible)`.
    /// Each entry: `(self_type_segments, self_type_visible,
    /// is_visible_trait_impl)`. The third flag is `true` iff this
    /// `impl Trait for X` is for a workspace-visible trait — needed
    /// because trait-impl items inherit the trait's visibility, not
    /// the impl-item's own `vis` modifier (which is `Inherited` in
    /// the typical `impl T for X { fn m() {} }` shape).
    impl_stack: Vec<(Vec<String>, bool, bool)>,
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
        self.impl_stack.last().map(|(segs, _, _)| segs.clone())
    }

    fn current_impl_visible(&self) -> bool {
        self.impl_stack.last().map(|(_, v, _)| *v).unwrap_or(false)
    }

    fn current_impl_is_visible_trait(&self) -> bool {
        self.impl_stack.last().map(|(_, _, t)| *t).unwrap_or(false)
    }

    fn record_fn(
        &mut self,
        name: String,
        line: usize,
        body: &'ast syn::Block,
        sig: &'ast syn::Signature,
        attrs: &[syn::Attribute],
    ) {
        self.found.push(PubFnInfo {
            file: self.file_path.clone(),
            fn_name: name,
            line,
            body,
            signature_params: extract_signature_params(sig),
            self_type: self.current_self_type(),
            mod_stack: self.mod_stack.clone(),
            deprecated: has_deprecated_attribute(attrs),
        });
    }
}

/// True iff the `#[test]` / `#[cfg(test)]` attribute set would make
/// this fn a test-harness item (excluded from the check).
fn is_test_fn(attrs: &[syn::Attribute]) -> bool {
    has_test_attr(attrs) || has_cfg_test(attrs)
}

/// True iff the attribute set contains `#[deprecated]` in any of its
/// three forms: bare `#[deprecated]`, `#[deprecated = "..."]`, or
/// `#[deprecated(since = "...", note = "...")]`. Operation: path-ident
/// probe.
fn has_deprecated_attribute(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("deprecated"))
}

/// True iff any attribute's last path segment matches an entry in
/// `promoted`. Used to lift macro-attributed private fns onto the
/// adapter-handler surface (`#[tool]` / `#[handler]` / framework
/// equivalents). Operation: per-attribute leaf-ident probe.
fn has_promoted_attribute(attrs: &[syn::Attribute], promoted: &HashSet<String>) -> bool {
    if promoted.is_empty() {
        return false;
    }
    attrs.iter().any(|a| {
        a.path()
            .segments
            .last()
            .is_some_and(|s| promoted.contains(&s.ident.to_string()))
    })
}

impl<'ast, 'vis> Visit<'ast> for PubFnCollector<'ast, 'vis> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let surface =
            is_visible(&node.vis) || has_promoted_attribute(&node.attrs, self.promoted_attributes);
        if self.enclosing_mod_visible && surface && !is_test_fn(&node.attrs) {
            let line = syn::spanned::Spanned::span(&node.sig.ident).start().line;
            let name = node.sig.ident.to_string();
            self.record_fn(name, line, &node.block, &node.sig, &node.attrs);
        }
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        // Skip `#[cfg(test)] impl X { … }` blocks entirely — the cfg
        // attribute lives on the impl block, child methods have no
        // attrs of their own. Without this guard test-only methods
        // would enter the target pub-fn surface and force adapter
        // coverage checks (Check B/D) for items that disappear in
        // production builds. Mirror of the same guard in
        // `file_fn_collector::visit_item_impl`.
        if has_cfg_test(&node.attrs) {
            return;
        }
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
        let scope = CanonScope {
            file: self.file,
            mod_stack: &self.mod_stack,
            reexports: self.reexports,
        };
        let canonical_segs = resolve_impl_self_type(&node.self_ty, &scope).unwrap_or_default();
        let visible = !canonical_segs.is_empty()
            && self.visible_canonicals.contains(&canonical_segs.join("::"));
        let is_visible_trait_impl =
            is_impl_for_visible_trait(node, &scope, self.visible_canonicals);
        self.impl_stack
            .push((canonical_segs, visible, is_visible_trait_impl));
        syn::visit::visit_item_impl(self, node);
        self.impl_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        // Trait-impl items inherit the trait's visibility for `node.vis`
        // — `fn m() {}` inside `impl PubTrait for X { ... }` is part of
        // the public surface even though `node.vis == Inherited`. The
        // self-type still gates membership: a method on a private type
        // is never registered, since dispatch into private impls is
        // collapsed to the trait-method anchor (`<PubTrait>::<m>`) and
        // the impl-method canonical never appears as a touchpoint.
        // Promoted attributes (rmcp `#[tool]`, axum `#[handler]`, …)
        // also lift a private impl method onto the handler surface so
        // proc-macro-generated dispatch surfaces are picked up
        // pre-expansion.
        let method_visible = is_visible(&node.vis)
            || self.current_impl_is_visible_trait()
            || has_promoted_attribute(&node.attrs, self.promoted_attributes);
        if self.current_impl_visible() && method_visible && !is_test_fn(&node.attrs) {
            let line = syn::spanned::Spanned::span(&node.sig.ident).start().line;
            let name = node.sig.ident.to_string();
            self.record_fn(name, line, &node.block, &node.sig, &node.attrs);
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

/// True iff `node` is `impl Trait for X { … }` and the resolved trait
/// canonical is in the workspace `visible_canonicals` set. The trait's
/// visibility carries through to its impl items — `fn m()` inside
/// `impl PubTrait for X {}` is part of the public surface even when
/// the impl-item `vis` is the syntactic `Inherited`.
fn is_impl_for_visible_trait(
    node: &syn::ItemImpl,
    scope: &CanonScope<'_>,
    visible_canonicals: &HashSet<String>,
) -> bool {
    use crate::adapters::analyzers::architecture::call_parity_rule::bindings::canonicalise_workspace_path;
    let Some((_, trait_path, _)) = node.trait_.as_ref() else {
        return false;
    };
    let segs: Vec<String> = trait_path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    // Use-site gate: `impl ::ext::Trait for X` doesn't refer to a
    // workspace trait, even if a same-named workspace trait exists.
    let Some(canonical) =
        canonicalise_workspace_path(&segs, trait_path.leading_colon.is_some(), scope)
    else {
        return false;
    };
    visible_canonicals.contains(&canonical.join("::"))
}
