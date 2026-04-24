//! Type-alias collection.
//!
//! For every top-level `type Alias = Target;` in the workspace, record
//! `canonical_Alias → Target` as a `syn::Type` clone. The inference
//! engine expands these on the fly when `resolve_type` encounters a
//! path whose canonical matches a recorded alias — useful for
//! `type Db = Arc<RwLock<Store>>` style indirection that otherwise
//! leaves user handlers unresolved.

use super::{canonical_type_key, BuildContext, WorkspaceTypeIndex};
use crate::adapters::shared::cfg_test::has_cfg_test;
use syn::visit::Visit;

/// Walk `ast` and populate `index.type_aliases`. Integration.
pub(super) fn collect_from_file(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    ast: &syn::File,
) {
    let mut collector = AliasCollector { index, ctx };
    collector.visit_file(ast);
}

struct AliasCollector<'i, 'c> {
    index: &'i mut WorkspaceTypeIndex,
    ctx: &'c BuildContext<'c>,
}

impl<'ast, 'i, 'c> Visit<'ast> for AliasCollector<'i, 'c> {
    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        let canonical = canonical_type_key(&[node.ident.to_string()], self.ctx);
        self.index
            .type_aliases
            .insert(canonical, (*node.ty).clone());
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_impl(&mut self, _: &'ast syn::ItemImpl) {
        // Type aliases inside impl blocks are `ImplItem::Type`, not
        // `Item::Type` — separate concern, not handled here.
    }
}
