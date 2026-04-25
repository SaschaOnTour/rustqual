//! Type-alias collection.
//!
//! For every top-level `type Alias<P1, P2, …> = Target;` in the
//! workspace, record `canonical_Alias → AliasDef { params, target,
//! decl_file, decl_mod_stack }`. Use-sites substitute generic args
//! into `target` and resolve the result against the alias's *own*
//! declaring scope — file-level `use crate::Store; type Repo =
//! Arc<Store>;` only resolves `Store` correctly if the resolver
//! consults the decl-site's alias map.

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
        self.index.type_aliases.insert(
            canonical,
            super::AliasDef {
                params,
                target: (*node.ty).clone(),
                decl_file: self.ctx.file.path.to_string(),
                decl_mod_stack: self.mod_stack.clone(),
            },
        );
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
