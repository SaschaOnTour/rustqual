//! Type-alias collection.
//!
//! For every top-level `type Alias<P1, P2, …> = Target;` in the
//! workspace, record `canonical_Alias → (params, Target)` as
//! `(Vec<String>, syn::Type)`. The generic parameter names are kept
//! so use-sites like `Alias<ArgA, ArgB>` can substitute them into
//! `Target` before resolution — without that, generic aliases like
//! `type AppResult<T> = Result<T, Error>` would cache `Result<T,
//! Error>` with `T` unbound and downstream `.unwrap()` would return
//! `Opaque`.

use super::{canonical_type_key, BuildContext, WorkspaceTypeIndex};
use crate::adapters::shared::cfg_test::has_cfg_test;
use syn::visit::Visit;

/// Walk `ast` and populate `index.type_aliases`. Integration.
pub(super) fn collect_from_file(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    ast: &syn::File,
) {
    let mut collector = AliasCollector {
        index,
        ctx,
        mod_stack: Vec::new(),
    };
    collector.visit_file(ast);
}

struct AliasCollector<'i, 'c> {
    index: &'i mut WorkspaceTypeIndex,
    ctx: &'c BuildContext<'c>,
    mod_stack: Vec<String>,
}

impl<'ast, 'i, 'c> Visit<'ast> for AliasCollector<'i, 'c> {
    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        let canonical = canonical_type_key(&[node.ident.to_string()], self.ctx, &self.mod_stack);
        let params: Vec<String> = node
            .generics
            .params
            .iter()
            .filter_map(|p| match p {
                syn::GenericParam::Type(t) => Some(t.ident.to_string()),
                _ => None,
            })
            .collect();
        self.index
            .type_aliases
            .insert(canonical, (params, (*node.ty).clone()));
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        self.mod_stack.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.mod_stack.pop();
    }

    fn visit_item_impl(&mut self, _: &'ast syn::ItemImpl) {
        // Type aliases inside impl blocks are `ImplItem::Type`, not
        // `Item::Type` — separate concern, not handled here.
    }
}
