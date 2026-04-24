//! Free-fn return-type collection.
//!
//! For every top-level `fn f(...) -> R` in the workspace (including
//! non-cfg-test inline modules), record `canonical_f → CanonicalType(R)`.
//! Methods (fns inside `impl` blocks) are indexed by `methods.rs`, not
//! here. Fns without an explicit return type, test fns, and `Opaque`
//! returns are skipped.

use super::super::canonical::CanonicalType;
use super::super::resolve::resolve_type;
use super::{resolve_ctx_from_build, BuildContext, WorkspaceTypeIndex};
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::shared::cfg_test::has_cfg_test;
use syn::visit::Visit;

/// Walk `ast` and populate `index.fn_returns`. Integration.
pub(super) fn collect_from_file(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    ast: &syn::File,
) {
    let mut collector = FnCollector { index, ctx };
    collector.visit_file(ast);
}

struct FnCollector<'i, 'c> {
    index: &'i mut WorkspaceTypeIndex,
    ctx: &'c BuildContext<'c>,
}

impl<'ast, 'i, 'c> Visit<'ast> for FnCollector<'i, 'c> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        record_fn(self.index, self.ctx, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_impl(&mut self, _: &'ast syn::ItemImpl) {
        // Methods live in the methods.rs collector, not here.
    }
}

/// Record one free fn's return type. Operation. Own calls hidden.
fn record_fn(index: &mut WorkspaceTypeIndex, ctx: &BuildContext<'_>, node: &syn::ItemFn) {
    let resolve = |ty: &syn::Type| resolve_type(ty, &resolve_ctx_from_build(ctx));
    let syn::ReturnType::Type(_, ret_ty) = &node.sig.output else {
        return;
    };
    let ret = resolve(ret_ty);
    if matches!(ret, CanonicalType::Opaque) {
        return;
    }
    let canonical = canonical_fn_name(&node.sig.ident.to_string(), ctx);
    index.fn_returns.insert(canonical, ret);
}

/// Build `crate::<file-module>::<fn_ident>`. Operation: string construction.
fn canonical_fn_name(fn_ident: &str, ctx: &BuildContext<'_>) -> String {
    let mut segs: Vec<String> = vec!["crate".to_string()];
    segs.extend(file_to_module_segments(ctx.path));
    segs.push(fn_ident.to_string());
    segs.join("::")
}
