//! Per-file syn-visitor that records every fn's canonical name and the
//! canonical callees its body emits, contributing one node + N edges to
//! the workspace `CallGraph`.
//!
//! Keeps the impl-block and inline-mod stack so methods record under
//! `crate::<file>::<mod_path>::<Type>::<method>` and free fns under
//! `crate::<file>::<mod_path>::<fn>`. cfg-test items are skipped at the
//! visitor level so the resulting graph never contains test-only nodes.

use super::bindings::CanonScope;
use super::calls::{collect_canonical_calls, FnContext};
use super::local_symbols::FileScope;
use super::signature_params::extract_signature_params;
use super::type_infer::WorkspaceTypeIndex;
use super::workspace_graph::{canonical_fn_name, resolve_impl_self_type, CallGraph};
use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use std::collections::HashMap;
use syn::visit::Visit;

pub(super) struct FileFnCollector<'a> {
    pub file: &'a FileScope<'a>,
    pub workspace_files: &'a HashMap<String, FileScope<'a>>,
    pub type_index: &'a WorkspaceTypeIndex,
    /// `None` marks an unresolved self-type (trait object, `&T`, tuple)
    /// whose methods we must not record.
    pub impl_type_stack: Vec<Option<Vec<String>>>,
    /// Enclosing inline-mod names so fns inside `mod inner { ... }`
    /// record under `crate::<file>::inner::fn`.
    pub mod_stack: Vec<String>,
    pub graph: &'a mut CallGraph,
}

impl<'a> FileFnCollector<'a> {
    fn record_fn<'ast>(
        &mut self,
        fn_name: &str,
        sig: &'ast syn::Signature,
        body: &'ast syn::Block,
    ) {
        let self_type = match self.impl_type_stack.last() {
            // Free fn (no enclosing impl).
            None => None,
            // Resolved impl — use its canonical self-type.
            Some(Some(segs)) => Some(segs.clone()),
            // Unresolved impl (trait object / reference receiver) —
            // don't record; see `resolve_impl_self_type`'s doc.
            Some(None) => return,
        };
        let canonical = canonical_fn_name(
            self.file.path,
            self_type.as_deref(),
            &self.mod_stack,
            fn_name,
        );
        let ctx = FnContext {
            file: self.file,
            mod_stack: &self.mod_stack,
            body,
            signature_params: extract_signature_params(sig),
            self_type,
            workspace_index: Some(self.type_index),
            workspace_files: Some(self.workspace_files),
        };
        let calls = collect_canonical_calls(&ctx);
        self.graph.add_node(&canonical);
        for callee in calls {
            self.graph.add_edge(&canonical, &callee);
        }
    }
}

impl<'a, 'ast> Visit<'ast> for FileFnCollector<'a> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if has_cfg_test(&node.attrs) || has_test_attr(&node.attrs) {
            return;
        }
        let name = node.sig.ident.to_string();
        self.record_fn(&name, &node.sig, &node.block);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        // Skip `#[cfg(test)] impl X { … }` blocks entirely — the cfg
        // attribute lives on the impl block, child methods have no
        // attrs of their own, so without this guard test-only methods
        // would leak into the production call graph as fake target
        // nodes (Check A would treat them as adapter delegation, Check
        // B/D would inspect a phantom impl-method canonical that
        // disappears in `cargo build` proper).
        if has_cfg_test(&node.attrs) {
            return;
        }
        // Canonicalise the impl's self-type through the file's alias
        // map so `use crate::app::Session; impl Session { ... }` and
        // `impl Session { ... }` in `src/app/session.rs` both produce
        // the same `crate::app::Session` prefix the call collector sees
        // from receiver-tracked method calls. `None` means the
        // self-type isn't a plain path (trait object, `&T`, tuple) —
        // `record_fn` skips method recording for those impls.
        let resolved = resolve_impl_self_type(
            &node.self_ty,
            &CanonScope {
                file: self.file,
                mod_stack: &self.mod_stack,
            },
        );
        self.impl_type_stack.push(resolved);
        syn::visit::visit_item_impl(self, node);
        self.impl_type_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if has_cfg_test(&node.attrs) || has_test_attr(&node.attrs) {
            return;
        }
        let name = node.sig.ident.to_string();
        self.record_fn(&name, &node.sig, &node.block);
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        // Skip inline `#[cfg(test)] mod tests { ... }` blocks entirely.
        // Their fns are test-only and must not pollute the call graph
        // (Check B could otherwise count a test as adapter coverage).
        if has_cfg_test(&node.attrs) {
            return;
        }
        self.mod_stack.push(node.ident.to_string());
        syn::visit::visit_item_mod(self, node);
        self.mod_stack.pop();
    }
}
