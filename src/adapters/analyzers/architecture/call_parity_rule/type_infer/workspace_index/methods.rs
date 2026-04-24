//! Method-return-type collection.
//!
//! For every `impl T { fn method(...) -> R }` (inherent or trait impl)
//! in the workspace, record `(canonical_T, method_name) → CanonicalType(R)`.
//!
//! Canonical-T keys match what `resolve_type` produces for a `Path`
//! variant: `crate::<file-module>::<ImplTypeSegments>`. So when
//! inference later calls `index.method_return(&path.join("::"), "m")`,
//! the lookup hits.
//!
//! Methods without an explicit return type (`fn m()` → `()`) are not
//! indexed — `()` carries no resolution power. Test methods
//! (`#[cfg(test)]` / `#[test]`) are skipped.

use super::super::canonical::CanonicalType;
use super::super::resolve::resolve_type;
use super::{canonical_type_key, resolve_ctx_from_build, BuildContext, WorkspaceTypeIndex};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::resolve_impl_self_type;
use crate::adapters::shared::cfg_test::has_cfg_test;
use syn::visit::Visit;

/// Walk `ast` and populate `index.method_returns`. Integration: delegates
/// to the nested visitor.
pub(super) fn collect_from_file(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    ast: &syn::File,
) {
    let mut collector = MethodCollector {
        index,
        ctx,
        impl_stack: Vec::new(),
        mod_stack: Vec::new(),
    };
    collector.visit_file(ast);
}

struct MethodCollector<'i, 'c> {
    index: &'i mut WorkspaceTypeIndex,
    ctx: &'c BuildContext<'c>,
    /// Stack of enclosing impl-block canonical self-types. `None` for
    /// unresolved (trait object, tuple receiver) — methods under those
    /// impls aren't indexed because the receiver type can't be named.
    impl_stack: Vec<Option<Vec<String>>>,
    /// Stack of enclosing inline `mod inner { ... }` block names so
    /// methods declared inside them key as
    /// `crate::<file>::inner::Type::method`.
    mod_stack: Vec<String>,
}

impl<'ast, 'i, 'c> Visit<'ast> for MethodCollector<'i, 'c> {
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        let resolved = resolve_impl_self_type(
            &node.self_ty,
            self.ctx.alias_map,
            self.ctx.local_symbols,
            self.ctx.crate_root_modules,
            self.ctx.path,
        );
        self.impl_stack.push(resolved);
        syn::visit::visit_item_impl(self, node);
        self.impl_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        record_method(
            self.index,
            self.ctx,
            &self.impl_stack,
            &self.mod_stack,
            node,
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
}

/// Record a single method's return type, keyed on the enclosing impl's
/// canonical self-type. `async fn m() -> T` is treated as returning
/// `Future<Output = T>` to match rustc's desugaring.
/// Operation. Own calls hidden in closures.
fn record_method(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    impl_stack: &[Option<Vec<String>>],
    mod_stack: &[String],
    node: &syn::ImplItemFn,
) {
    let resolve = |ty: &syn::Type| resolve_type(ty, &resolve_ctx_from_build(ctx));
    let canon = |segs: &[String]| canonical_type_key(segs, ctx, mod_stack);
    let Some(Some(impl_segs)) = impl_stack.last() else {
        return;
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
    let receiver_canonical = canon(impl_segs);
    let method_name = node.sig.ident.to_string();
    index
        .method_returns
        .insert((receiver_canonical, method_name), ret);
}
