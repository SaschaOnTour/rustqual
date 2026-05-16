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
use super::super::self_subst::substitute_bare_self;
use super::{canonical_type_key, resolve_ctx_with_generics, BuildContext, WorkspaceTypeIndex};
use crate::adapters::analyzers::architecture::call_parity_rule::bindings::CanonScope;
use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::{
    extract_generics, merge_generic_params,
};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::resolve_impl_self_type;
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use std::collections::HashMap;
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

/// Per-impl-block frame on the visitor's stack: the resolved self-type
/// segments and the impl-level generic params. `self_ty` is `None` when
/// the impl's self-type can't be canonicalised (trait object / tuple
/// receiver) — methods under such an impl aren't indexed.
struct ImplFrame {
    self_ty: Option<Vec<String>>,
    generics: Vec<(String, Vec<Vec<String>>)>,
}

struct MethodCollector<'i, 'c> {
    index: &'i mut WorkspaceTypeIndex,
    ctx: &'c BuildContext<'c>,
    impl_stack: Vec<ImplFrame>,
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
        let self_ty = resolve_impl_self_type(
            &node.self_ty,
            &CanonScope {
                file: self.ctx.file,
                mod_stack: &self.mod_stack,
            },
        );
        self.impl_stack.push(ImplFrame {
            self_ty,
            generics: extract_generics(&node.generics),
        });
        syn::visit::visit_item_impl(self, node);
        self.impl_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if has_cfg_test(&node.attrs) || has_test_attr(&node.attrs) {
            return;
        }
        let Some(frame) = self.impl_stack.last() else {
            return;
        };
        record_method(self.index, self.ctx, frame, &self.mod_stack, node);
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
/// `Future<Output = T>` to match rustc's desugaring. Impl-level and
/// method-level generic params are merged and threaded into the
/// resolve context so a return type spelled `Q` shadows any workspace
/// symbol named `Q`. Operation. Own calls hidden in closures.
fn record_method(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    frame: &ImplFrame,
    mod_stack: &[String],
    node: &syn::ImplItemFn,
) {
    let Some(impl_segs) = frame.self_ty.as_deref() else {
        return;
    };
    let syn::ReturnType::Type(_, ret_ty) = &node.sig.output else {
        return;
    };
    let merged: HashMap<String, Vec<Vec<String>>> =
        merge_generic_params(frame.generics.clone(), extract_generics(&node.sig.generics))
            .into_iter()
            .collect();
    let inner = resolve_method_return(ret_ty, impl_segs, ctx, mod_stack, &merged);
    if matches!(inner, CanonicalType::Opaque) {
        return;
    }
    let ret = if node.sig.asyncness.is_some() {
        CanonicalType::Future(Box::new(inner))
    } else {
        inner
    };
    let receiver_canonical = canonical_type_key(impl_segs, ctx, mod_stack);
    let method_name = node.sig.ident.to_string();
    index.insert_method_return(receiver_canonical, method_name, ret);
}

/// Resolve a method's return type, substituting bare `Self` with the
/// enclosing impl's canonical self-type. Wrapper return types
/// (`Result<Self, E>`, `Option<Self>`, `Vec<Self>`) project the inner
/// `Self` correctly. Multi-segment paths (`Self::Output`,
/// `Self::Inner`) keep the raw segments and resolve as before —
/// associated-type resolution stays out of scope.
fn resolve_method_return(
    ret_ty: &syn::Type,
    impl_segs: &[String],
    ctx: &BuildContext<'_>,
    mod_stack: &[String],
    generic_params: &HashMap<String, Vec<Vec<String>>>,
) -> CanonicalType {
    let substituted = substitute_bare_self(ret_ty, impl_segs);
    resolve_type(
        &substituted,
        &resolve_ctx_with_generics(ctx, mod_stack, Some(generic_params)),
    )
}
