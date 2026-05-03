//! Trait-definition + trait-impl collection.
//!
//! Populates two maps on `WorkspaceTypeIndex`:
//!
//! - `trait_methods`: `trait_canonical → {method_name, …}` — the set
//!   of methods each trait declares. Used so trait-dispatch resolution
//!   only fires for methods that actually belong to the trait
//!   (`dyn Trait.unrelated_method()` stays unresolved).
//! - `trait_impls`: `trait_canonical → [impl_type_canonical, …]` —
//!   every workspace-local impl of a trait. Stage 2 trait-dispatch
//!   over-approximates by recording an edge to every impl's method.

use super::{canonical_type_key, BuildContext, WorkspaceTypeIndex};
use crate::adapters::analyzers::architecture::call_parity_rule::bindings::{
    canonicalise_type_segments_in_scope, CanonScope,
};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::resolve_impl_self_type;
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use std::collections::HashSet;
use syn::visit::Visit;

/// Walk `ast` and populate both `trait_methods` and `trait_impls` on
/// `index`. Integration.
pub(super) fn collect_from_file(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    ast: &syn::File,
) {
    let mut collector = TraitCollector {
        index,
        ctx,
        mod_stack: Vec::new(),
    };
    collector.visit_file(ast);
}

struct TraitCollector<'i, 'c> {
    index: &'i mut WorkspaceTypeIndex,
    ctx: &'c BuildContext<'c>,
    mod_stack: Vec<String>,
}

impl<'ast, 'i, 'c> Visit<'ast> for TraitCollector<'i, 'c> {
    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        record_trait_methods(self.index, self.ctx, &self.mod_stack, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        record_trait_impl(self.index, self.ctx, &self.mod_stack, node);
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

/// For a `trait T { fn m(…); fn n(…); }` record
/// `trait_methods[canonical_T] = {"m", "n"}`. Operation.
fn record_trait_methods(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    mod_stack: &[String],
    node: &syn::ItemTrait,
) {
    let canonical = canonical_name(&node.ident.to_string(), ctx, mod_stack);
    let methods: HashSet<String> = node
        .items
        .iter()
        .filter_map(|item| match item {
            syn::TraitItem::Fn(f) => Some(f.sig.ident.to_string()),
            _ => None,
        })
        .collect();
    if !methods.is_empty() {
        index.trait_methods.insert(canonical, methods);
    }
}

/// For `impl Trait for X { … }` record `trait_impls[canonical_Trait]`
/// gaining `canonical_X`. Inherent impls (without `trait_`) are handled
/// by `methods.rs`, not here. Operation: delegated canonicalisation.
fn record_trait_impl(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    mod_stack: &[String],
    node: &syn::ItemImpl,
) {
    let Some((_, trait_path, _)) = &node.trait_ else {
        return;
    };
    let trait_canonical = resolve_trait_path(trait_path, ctx, mod_stack);
    let Some(trait_canonical) = trait_canonical else {
        return;
    };
    let impl_type_canonical = resolve_impl_self_type(
        &node.self_ty,
        &CanonScope {
            file: ctx.file,
            mod_stack,
        },
    );
    let Some(impl_segs) = impl_type_canonical else {
        return;
    };
    let impl_canonical = canonical_impl_type(&impl_segs, ctx, mod_stack);
    // Skip cfg-test / #[test]-gated impl items so `dispatch_edges` does
    // not emit a phantom touchpoint for a test-only override of a
    // default method (the workspace graph and `method_returns` index
    // both skip those items, so the production call still inherits
    // the trait default and stays unresolved).
    let overridden: std::collections::HashSet<String> = node
        .items
        .iter()
        .filter_map(|item| match item {
            syn::ImplItem::Fn(f) if !has_cfg_test(&f.attrs) && !has_test_attr(&f.attrs) => {
                Some(f.sig.ident.to_string())
            }
            _ => None,
        })
        .collect();
    index
        .trait_impls
        .entry(trait_canonical.clone())
        .or_default()
        .push(impl_canonical.clone());
    index
        .trait_impl_overrides
        .entry(trait_canonical)
        .or_default()
        .insert(impl_canonical, overridden);
}

/// Resolve a trait path (the `T` in `impl T for X`) to its canonical
/// crate-rooted form via the shared canonicalisation pipeline. Mod
/// scope is honoured so a single-ident trait declared inside an inline
/// mod resolves to `crate::<file>::<mod>::Trait`.
/// Operation: flatten + delegate.
fn resolve_trait_path(
    path: &syn::Path,
    ctx: &BuildContext<'_>,
    mod_stack: &[String],
) -> Option<String> {
    let segs: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let resolved = canonicalise_type_segments_in_scope(
        &segs,
        &CanonScope {
            file: ctx.file,
            mod_stack,
        },
    )?;
    Some(resolved.join("::"))
}

/// `crate::<file-module>::<inline-mods>::<trait_or_type_ident>`.
/// Operation.
fn canonical_name(ident: &str, ctx: &BuildContext<'_>, mod_stack: &[String]) -> String {
    let mut segs: Vec<String> = vec!["crate".to_string()];
    segs.extend(file_to_module_segments(ctx.file.path));
    segs.extend(mod_stack.iter().cloned());
    segs.push(ident.to_string());
    segs.join("::")
}

/// Same shape as methods.rs — prefix impl-type segs with `crate::
/// <file-module>::<inline-mods>::` unless the impl path is already
/// crate-rooted. Operation: delegate to the shared `canonical_type_key`.
fn canonical_impl_type(
    impl_segs: &[String],
    ctx: &BuildContext<'_>,
    mod_stack: &[String],
) -> String {
    canonical_type_key(impl_segs, ctx, mod_stack)
}
