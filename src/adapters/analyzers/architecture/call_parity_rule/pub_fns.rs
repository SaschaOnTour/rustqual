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

use super::bindings::{canonicalise_type_segments_in_scope, CanonScope};
use super::local_symbols::{collect_local_symbols_scoped, FileScope, LocalSymbols};
use super::signature_params::extract_signature_params;
use super::workspace_graph::{collect_crate_root_modules, resolve_impl_self_type};
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use crate::adapters::shared::use_tree::gather_alias_map_scoped;
use crate::adapters::shared::use_tree::ScopedAliasMap;
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
    let crate_root_modules = collect_crate_root_modules(files);
    let visible_canonicals = collect_visible_type_canonicals_workspace(
        files,
        cfg_test_files,
        aliases_per_file,
        &crate_root_modules,
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

/// Collect every publicly named type's canonical path across the
/// whole non-test workspace. The set members are
/// `crate::<file_modules>::<mod_stack>::<ident>` joined by `::`.
/// Compared canonically against the impl self-type's resolved path,
/// so two distinct types sharing a short ident (`api::Session` vs
/// `internal::Session`) don't collide, and `mod private { pub struct
/// Hidden; } pub use private::Hidden;` correctly registers the
/// source-canonical of `Hidden` via the re-export.
///
/// Impls on the same canonical path get counted as visible regardless
/// of which file the impl lives in — so `pub struct Session` in one
/// file and `impl crate::app::Session` in a companion file both
/// resolve to the same canonical and register together.
/// Integration: per-file delegate to recursive collector.
fn collect_visible_type_canonicals_workspace(
    files: &[(&str, &syn::File)],
    cfg_test_files: &HashSet<String>,
    aliases_per_file: &HashMap<String, HashMap<String, Vec<String>>>,
    crate_root_modules: &HashSet<String>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    let empty_aliases = HashMap::new();
    for (path, ast) in files {
        if cfg_test_files.contains(*path) {
            continue;
        }
        let alias_map = aliases_per_file.get(*path).unwrap_or(&empty_aliases);
        let LocalSymbols { flat, by_name } = collect_local_symbols_scoped(ast);
        let aliases_per_scope = gather_alias_map_scoped(ast);
        let file_scope = FileScope {
            path,
            alias_map,
            aliases_per_scope: &aliases_per_scope,
            local_symbols: &flat,
            local_decl_scopes: &by_name,
            crate_root_modules,
        };
        collect_visible_type_canonicals_in_items(&ast.items, &[], &file_scope, &mut out);
    }
    out
}

/// Walk a slice of items, inserting publicly named types' canonical
/// paths and recursing into non-cfg-test, visible inline mods. `pub
/// use` items resolve their leaves through the workspace alias /
/// local-symbol pipeline so re-exported source-canonicals enter the
/// set even when the source module itself is private. Glob re-exports
/// (`pub use foo::*`) are intentionally skipped — without expanding
/// the source module we can't statically tell which idents leak.
/// Operation: closure-hidden recursion through nested `mod` blocks.
// qual:recursive
fn collect_visible_type_canonicals_in_items(
    items: &[syn::Item],
    mod_stack: &[String],
    file_scope: &FileScope<'_>,
    out: &mut HashSet<String>,
) {
    let recurse = |inner: &[syn::Item], next: &[String], out: &mut HashSet<String>| {
        collect_visible_type_canonicals_in_items(inner, next, file_scope, out);
    };
    let add_decl = |ident: &syn::Ident, out: &mut HashSet<String>| {
        out.insert(canonical_for_decl(
            file_scope.path,
            mod_stack,
            &ident.to_string(),
        ));
    };
    let collect_use = |tree: &syn::UseTree, out: &mut HashSet<String>| {
        walk_use_tree_canonicals(tree, &mut Vec::new(), file_scope, mod_stack, out);
    };
    for item in items {
        match item {
            syn::Item::Struct(s) if is_visible(&s.vis) => add_decl(&s.ident, out),
            syn::Item::Enum(e) if is_visible(&e.vis) => add_decl(&e.ident, out),
            syn::Item::Union(u) if is_visible(&u.vis) => add_decl(&u.ident, out),
            syn::Item::Trait(t) if is_visible(&t.vis) => add_decl(&t.ident, out),
            syn::Item::Type(t) if is_visible(&t.vis) => add_decl(&t.ident, out),
            syn::Item::Use(u) if is_visible(&u.vis) => collect_use(&u.tree, out),
            syn::Item::Mod(m) if is_visible(&m.vis) && !has_cfg_test(&m.attrs) => {
                if let Some((_, inner)) = m.content.as_ref() {
                    let mut next = mod_stack.to_vec();
                    next.push(m.ident.to_string());
                    recurse(inner, &next, out);
                }
            }
            _ => {}
        }
    }
}

/// Build `crate::<file_modules>::<mod_stack>::<ident>` joined as a
/// single string — the canonical key both `visible_canonicals` and
/// `resolve_impl_self_type` agree on. Operation: pure string assembly.
fn canonical_for_decl(file_path: &str, mod_stack: &[String], ident: &str) -> String {
    let mut segs = vec!["crate".to_string()];
    segs.extend(file_to_module_segments(file_path));
    segs.extend(mod_stack.iter().cloned());
    segs.push(ident.to_string());
    segs.join("::")
}

/// Recursive walk over a `pub use` tree, accumulating the path
/// segments as we descend so each leaf carries the full source path.
/// Each leaf is canonicalised through the workspace alias / local-
/// symbol pipeline; the result enters `visible_canonicals` so the
/// source type's canonical is recognised even if its module is
/// private. The leaf's *source* ident is what gets resolved — the
/// rename only affects how callers name the type, while impl methods
/// still record under the source-canonical. Operation: closure-hidden
/// descent into nested `Group`s and `Path`s.
// qual:recursive
fn walk_use_tree_canonicals(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    file_scope: &FileScope<'_>,
    mod_stack: &[String],
    out: &mut HashSet<String>,
) {
    let recurse = |sub: &syn::UseTree, prefix: &mut Vec<String>, out: &mut HashSet<String>| {
        walk_use_tree_canonicals(sub, prefix, file_scope, mod_stack, out);
    };
    let resolve = |segs: &[String], out: &mut HashSet<String>| {
        let scope = CanonScope {
            file: file_scope,
            mod_stack,
        };
        if let Some(canonical) = canonicalise_type_segments_in_scope(segs, &scope) {
            out.insert(canonical.join("::"));
        }
    };
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            recurse(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            prefix.push(n.ident.to_string());
            resolve(prefix, out);
            prefix.pop();
        }
        syn::UseTree::Rename(r) => {
            prefix.push(r.ident.to_string());
            resolve(prefix, out);
            prefix.pop();
        }
        syn::UseTree::Group(g) => {
            for sub in &g.items {
                recurse(sub, prefix, out);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
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

/// Visibility modifier counts as "visible for the check" iff it's
/// `pub`, `pub(crate)`, `pub(super)`, or `pub(in <path>)` for any
/// non-`self` path. `Inherited` and `pub(self)` / `pub(in self)`
/// (which Rust treats as equivalent to inherited visibility) both
/// stay out of scope. See D-5 for the rationale.
fn is_visible(vis: &Visibility) -> bool {
    match vis {
        Visibility::Inherited => false,
        Visibility::Restricted(r) => !is_self_restricted(&r.path),
        _ => true,
    }
}

/// True when a `pub(in path)` restriction targets `self` — i.e.
/// `pub(self)` or `pub(in self)`. Both compile to a single-segment
/// path of `self`. Operation.
fn is_self_restricted(path: &syn::Path) -> bool {
    path.leading_colon.is_none() && path.segments.len() == 1 && path.segments[0].ident == "self"
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
