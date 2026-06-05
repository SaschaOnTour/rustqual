//! Scope-stack management + signature/closure binding seeding.

use super::super::bindings::{
    canonical_from_type, extract_let_binding, normalize_alias_expansion, CanonScope,
};
use super::super::local_symbols::{scope_for_local, FileScope};
use super::super::type_infer::resolve::{resolve_type, ResolveContext};
use super::super::type_infer::self_subst::substitute_bare_self;
use super::super::type_infer::{
    extract_bindings, extract_for_bindings, infer_type, BindingLookup, CanonicalType, InferContext,
    WorkspaceTypeIndex,
};
use super::{
    bare, collect_pattern_idents, extract_pat_ident_name, method_unknown, parse_macro_tokens,
    CanonicalCallCollector, CollectorBindings, FnContext,
};
use crate::adapters::analyzers::architecture::forbidden_rule::{
    file_to_module_segments, resolve_to_crate_absolute_in,
};
use crate::adapters::shared::use_tree::AliasTarget;
use std::collections::{HashMap, HashSet};
use syn::spanned::Spanned;

impl<'a> CanonicalCallCollector<'a> {
    pub(super) fn new(ctx: &'a FnContext<'a>) -> Self {
        let self_type_canonical = ctx.self_type.as_ref().map(|segs| {
            // Qualified impl path (`impl crate::foo::Bar { ... }`) — use
            // as-is so Self::method canonicalises to `crate::foo::Bar::method`.
            if segs.first().map(|s| s.as_str()) == Some("crate") {
                return segs.clone();
            }
            let mut full = vec!["crate".to_string()];
            full.extend(file_to_module_segments(ctx.file.path));
            full.extend(ctx.mod_stack.iter().cloned());
            full.extend_from_slice(segs);
            full
        });
        Self {
            file: ctx.file,
            mod_stack: ctx.mod_stack,
            self_type_canonical,
            signature_params: ctx.signature_params.clone(),
            generic_params: ctx.generic_params.clone(),
            bindings: vec![HashMap::new()],
            non_path_bindings: vec![HashMap::new()],
            calls: HashSet::new(),
            workspace_index: ctx.workspace_index,
            workspace_files: ctx.workspace_files,
            reexports: ctx.reexports,
        }
    }

    pub(super) fn seed_signature_bindings(&mut self) {
        // `self` is `FnArg::Receiver` and never appears in
        // `signature_params`. Seed it explicitly so `self.helper()` and
        // `self.field.method()` route through `method_returns` /
        // `struct_fields` instead of collapsing to `<method>:…`.
        if let Some(self_canonical) = self.self_type_canonical.clone() {
            self.bindings[0].insert("self".to_string(), self_canonical);
        }
        let params = self.signature_params.clone();
        for (name, ty) in &params {
            // When workspace_index is available, use the full resolver:
            // it handles Stage-3 type-alias expansion, Stage-2 dyn Trait,
            // stdlib wrappers, and plain Path in one pass.
            if self.workspace_index.is_some() {
                self.seed_param_via_resolver(name, ty);
                continue;
            }
            // Legacy fast-path for unit-test fixtures without an index.
            if let Some(canonical) = canonical_from_type(
                ty,
                self.file.alias_map,
                self.file.local_symbols,
                self.file.crate_root_modules,
                self.file.path,
            ) {
                self.bindings[0].insert(name.clone(), canonical);
            }
        }
    }

    /// Install a signature-param binding using the full `resolve_type`
    /// pipeline. Path → legacy scope, wrappers / trait bounds →
    /// `non_path_bindings`, `Opaque` dropped. Always seeds frame 0
    /// because signature params live for the whole body walk.
    pub(super) fn seed_param_via_resolver(&mut self, name: &str, ty: &syn::Type) {
        match self.resolve_param_type(ty) {
            CanonicalType::Path(segs) => {
                self.bindings[0].insert(name.to_string(), segs);
            }
            CanonicalType::Opaque => {}
            other => {
                self.non_path_bindings[0].insert(name.to_string(), other);
            }
        }
    }

    /// Resolve a parameter / closure-arg type through the full
    /// scope-aware pipeline (alias expansion, transparent wrappers,
    /// trait-bound extraction, inline-mod resolution). Pre-substitutes
    /// bare `Self` with `self_type_canonical` so impl-body declarations
    /// like `fn merge(&self, other: Self)` and typed closure params
    /// resolve to the enclosing impl type. Used by both signature
    /// seeding and closure-param seeding.
    pub(super) fn resolve_param_type(&self, ty: &syn::Type) -> CanonicalType {
        let rctx = ResolveContext {
            file: self.file,
            mod_stack: self.mod_stack,
            type_aliases: self.workspace_index.map(|w| &w.type_aliases),
            transparent_wrappers: self.workspace_index.map(|w| &w.transparent_wrappers),
            workspace_files: self.workspace_files,
            alias_param_subs: None,
            generic_params: Some(&self.generic_params),
            reexports: self.reexports,
        };
        match self.self_type_canonical.as_deref() {
            Some(impl_segs) => resolve_type(&substitute_bare_self(ty, impl_segs), &rctx),
            None => resolve_type(ty, &rctx),
        }
    }

    pub(super) fn enter_scope(&mut self) {
        self.bindings.push(HashMap::new());
        self.non_path_bindings.push(HashMap::new());
    }

    pub(super) fn exit_scope(&mut self) {
        self.bindings.pop();
        self.non_path_bindings.pop();
    }

    /// Return the innermost binding scope. The stack is seeded non-empty
    /// in `new()` and only mutated via paired `enter_scope` / `exit_scope`
    /// calls, so `last_mut()` is always `Some`; fall back to index access
    /// to avoid panic-helper methods in production code.
    pub(super) fn current_scope_mut(&mut self) -> &mut HashMap<String, Vec<String>> {
        if self.bindings.is_empty() {
            self.bindings.push(HashMap::new());
        }
        let last = self.bindings.len() - 1;
        &mut self.bindings[last]
    }

    /// Parallel accessor for the non-path scope stack. Same invariants
    /// and fallback semantics as `current_scope_mut`.
    pub(super) fn current_non_path_scope_mut(&mut self) -> &mut HashMap<String, CanonicalType> {
        if self.non_path_bindings.is_empty() {
            self.non_path_bindings.push(HashMap::new());
        }
        let last = self.non_path_bindings.len() - 1;
        &mut self.non_path_bindings[last]
    }

    /// Install a binding in the path-scope and evict any stale entry
    /// for the same name in the non-path scope (a `let` that shadows a
    /// previous wrapper-typed binding with a plain Path binding).
    /// Operation.
    pub(super) fn install_path_binding(&mut self, name: String, segs: Vec<String>) {
        self.current_non_path_scope_mut().remove(&name);
        self.current_scope_mut().insert(name, segs);
    }

    /// Install a wrapper / trait-bound binding in the non-path scope
    /// and evict any stale Path binding for the same name (shadowing
    /// the other way). Operation.
    pub(super) fn install_non_path_binding(&mut self, name: String, ty: CanonicalType) {
        self.current_scope_mut().remove(&name);
        self.current_non_path_scope_mut().insert(name, ty);
    }

    /// Install closure parameter bindings. For `|x: T|` the type goes
    /// through the same scope-aware pipeline as signature params. For
    /// untyped or destructured patterns, every bound ident gets an
    /// `Opaque` tombstone — without this, an outer same-name binding
    /// could leak into the closure body and synthesize a stale edge.
    pub(super) fn install_closure_param(&mut self, pat: &syn::Pat) {
        if let syn::Pat::Type(pt) = pat {
            if let Some(name) = extract_pat_ident_name(pt.pat.as_ref()) {
                match self.resolve_param_type(&pt.ty) {
                    CanonicalType::Path(segs) => self.install_path_binding(name, segs),
                    other => self.install_non_path_binding(name, other),
                }
                return;
            }
        }
        let mut idents = Vec::new();
        collect_pattern_idents(pat, &mut idents);
        for name in idents {
            self.install_non_path_binding(name, CanonicalType::Opaque);
        }
    }
}
