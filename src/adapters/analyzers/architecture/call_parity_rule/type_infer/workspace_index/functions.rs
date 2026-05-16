//! Free-fn return-type collection.
//!
//! For every top-level `fn f(...) -> R` in the workspace (including
//! non-cfg-test inline modules), record `canonical_f → CanonicalType(R)`.
//! Methods (fns inside `impl` blocks) are indexed by `methods.rs`, not
//! here. Fns without an explicit return type, test fns, and `Opaque`
//! returns are skipped.

use super::super::canonical::CanonicalType;
use super::super::resolve::resolve_type;
use super::{resolve_ctx_with_generics, BuildContext, WorkspaceTypeIndex};
use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::item_canonical_generics;
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use syn::visit::Visit;

/// Walk `ast` and populate `index.fn_returns`. Integration.
pub(super) fn collect_from_file(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    ast: &syn::File,
) {
    let mut collector = FnCollector {
        index,
        ctx,
        mod_stack: Vec::new(),
    };
    collector.visit_file(ast);
}

struct FnCollector<'i, 'c> {
    index: &'i mut WorkspaceTypeIndex,
    ctx: &'c BuildContext<'c>,
    mod_stack: Vec<String>,
}

impl<'ast, 'i, 'c> Visit<'ast> for FnCollector<'i, 'c> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if has_cfg_test(&node.attrs) || has_test_attr(&node.attrs) {
            return;
        }
        record_fn(self.index, self.ctx, &self.mod_stack, node);
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
        // Methods live in the methods.rs collector, not here.
    }
}

/// Record one free fn's return type. `async fn foo() -> T` is treated
/// as returning `Future<Output = T>` to match rustc's desugaring so
/// downstream `.await` unwraps correctly. The fn's own generic params
/// are canonicalised and threaded into the resolve context so a return
/// type spelled `Q` shadows any workspace symbol named `Q`, and bounds
/// resolve to crate-rooted trait paths. The resulting `TraitBound` is
/// stored (so trait-anchor dispatch on the return value still works);
/// `infer_call` then prefers an explicit turbofish substitution over
/// the bound when both are available. Operation.
fn record_fn(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    mod_stack: &[String],
    node: &syn::ItemFn,
) {
    let generics = item_canonical_generics(&node.sig.generics, ctx.file, mod_stack);
    let resolve = |ty: &syn::Type| {
        resolve_type(
            ty,
            &resolve_ctx_with_generics(ctx, mod_stack, Some(&generics)),
        )
    };
    let syn::ReturnType::Type(_, ret_ty) = &node.sig.output else {
        return;
    };
    let inner = resolve(ret_ty);
    if matches!(inner, CanonicalType::Opaque) {
        return;
    }
    let ret = if node.sig.asyncness.is_some() {
        CanonicalType::Future(Box::new(inner))
    } else {
        inner
    };
    let canonical = canonical_fn_name(&node.sig.ident.to_string(), ctx, mod_stack);
    index.fn_returns.insert(canonical, ret);
}

/// Build `crate::<file-module>::<inline-mods>::<fn_ident>`. Operation:
/// string construction.
fn canonical_fn_name(fn_ident: &str, ctx: &BuildContext<'_>, mod_stack: &[String]) -> String {
    let mut segs: Vec<String> = vec!["crate".to_string()];
    segs.extend(file_to_module_segments(ctx.file.path));
    segs.extend(mod_stack.iter().cloned());
    segs.push(fn_ident.to_string());
    segs.join("::")
}
