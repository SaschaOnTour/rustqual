//! Trait-definition + trait-impl collection.
//!
//! Populates the trait-related maps on `WorkspaceTypeIndex`:
//!
//! - `trait_methods`: `trait_canonical → {method_name, …}` — the set
//!   of methods each trait declares. Gates trait-dispatch so
//!   `dyn Trait.unrelated_method()` stays unresolved.
//! - `trait_method_locations` + `trait_methods_with_default_body`:
//!   per-method side-tables populated via `trait_method_details` —
//!   carry source spans and default-body flags forward to
//!   `AnchorInfo`, so anchor findings get real source lines and the
//!   unified target-capability rule can distinguish callable defaults
//!   from pure signatures.
//! - `trait_impls`: `trait_canonical → [impl_type_canonical, …]` —
//!   every workspace-local impl. Feeds `AnchorInfo.impl_layers` (and
//!   `impl_method_canonicals`) so the boundary walker and Check B/D
//!   share one target-capability rule.
//! - `trait_impl_overrides`: `trait_canonical → {impl_type → {overridden, …}}`
//!   — which methods each impl actually defines (cfg-test items
//!   filtered). Used to project the "overriding impls" subset that
//!   `AnchorInfo.impl_method_canonicals` needs.

use super::{canonical_type_key, BuildContext, MethodLocation, WorkspaceTypeIndex};
use crate::adapters::analyzers::architecture::call_parity_rule::bindings::{
    canonicalise_type_segments_in_scope, CanonScope,
};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::resolve_impl_self_type;
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use std::collections::HashSet;
use syn::spanned::Spanned;
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

/// For a `trait T { fn m(…); fn n(…); }` record `trait_methods` plus
/// per-method side-tables (`trait_method_locations` for source spans,
/// `trait_methods_with_default_body` for default-body presence). One
/// walk over `node.items`; the side-tables carry through to
/// `AnchorInfo` so anchor findings get real source coordinates and the
/// unified target-capability rule can distinguish callable defaults
/// from pure signatures. Integration: one walk + per-fn detail
/// extraction.
fn record_trait_methods(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    mod_stack: &[String],
    node: &syn::ItemTrait,
) {
    let canonical = canonical_type_key(&[node.ident.to_string()], ctx, mod_stack);
    let mut methods: HashSet<String> = HashSet::new();
    for item in &node.items {
        let syn::TraitItem::Fn(f) = item else {
            continue;
        };
        let method = f.sig.ident.to_string();
        record_trait_method_details(index, ctx, &canonical, &method, f);
        methods.insert(method);
    }
    if !methods.is_empty() {
        index.trait_methods.insert(canonical, methods);
    }
}

/// Record one trait-method's side-table entries: source location (for
/// anchor finding spans) and default-body flag (for the unified
/// target-capability rule). Synthetic spans (`Span::call_site()`,
/// line=0) are skipped — downstream callers fall back to
/// canonical-derived path heuristics for those. Operation.
fn record_trait_method_details(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    trait_canonical: &str,
    method: &str,
    f: &syn::TraitItemFn,
) {
    let span = f.sig.ident.span().start();
    if span.line > 0 {
        index.trait_method_locations.insert(
            (trait_canonical.to_string(), method.to_string()),
            MethodLocation {
                file: ctx.file.path.to_string(),
                line: span.line,
                column: span.column,
            },
        );
    }
    if f.default.is_some() {
        index
            .trait_methods_with_default_body
            .insert((trait_canonical.to_string(), method.to_string()));
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
    let Some(trait_canonical) = resolve_trait_path(trait_path, ctx, mod_stack) else {
        return;
    };
    let scope = CanonScope {
        file: ctx.file,
        mod_stack,
    };
    let Some(impl_segs) = resolve_impl_self_type(&node.self_ty, &scope) else {
        return;
    };
    let impl_canonical = canonical_type_key(&impl_segs, ctx, mod_stack);
    let overridden = collect_overridden_method_names(node);
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

/// Collect the names of methods actually defined inside a trait impl
/// block, excluding `#[cfg(test)]` and `#[test]`-gated entries. The
/// workspace graph and `method_returns` index skip those items too,
/// so a test-only override doesn't fabricate a phantom touchpoint
/// (the production call inherits the trait default and stays
/// unresolved). Operation.
fn collect_overridden_method_names(node: &syn::ItemImpl) -> std::collections::HashSet<String> {
    node.items
        .iter()
        .filter_map(|item| match item {
            syn::ImplItem::Fn(f) if !has_cfg_test(&f.attrs) && !has_test_attr(&f.attrs) => {
                Some(f.sig.ident.to_string())
            }
            _ => None,
        })
        .collect()
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
