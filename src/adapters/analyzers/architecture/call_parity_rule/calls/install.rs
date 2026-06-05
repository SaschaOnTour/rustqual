//! let/for/destructure binding installation.

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
    /// `let x: T = …` — route the annotation through the full resolver
    /// when a workspace index is available so alias expansion + wrapper
    /// peeling + trait-bound extraction all apply. Returns `true` when
    /// a binding was installed. `false` when there's no workspace index,
    /// the pattern isn't a typed ident, or the annotation is `_`. An
    /// unresolvable type still installs an `Opaque` tombstone. Operation.
    pub(super) fn try_install_annotated_binding(&mut self, local: &syn::Local) -> bool {
        let Some(wi) = self.workspace_index else {
            return false;
        };
        let syn::Pat::Type(pt) = &local.pat else {
            return false;
        };
        let syn::Pat::Ident(pi) = pt.pat.as_ref() else {
            return false;
        };
        if matches!(pt.ty.as_ref(), syn::Type::Infer(_)) {
            return false;
        }
        let rctx = ResolveContext {
            file: self.file,
            mod_stack: self.mod_stack,
            type_aliases: Some(&wi.type_aliases),
            transparent_wrappers: Some(&wi.transparent_wrappers),
            workspace_files: self.workspace_files,
            alias_param_subs: None,
            generic_params: Some(&self.generic_params),
            reexports: self.reexports,
        };
        let name = pi.ident.to_string();
        let resolved = match self.self_type_canonical.as_deref() {
            Some(impl_segs) => {
                resolve_type(&substitute_bare_self(pt.ty.as_ref(), impl_segs), &rctx)
            }
            None => resolve_type(pt.ty.as_ref(), &rctx),
        };
        match resolved {
            CanonicalType::Path(segs) => self.install_path_binding(name, segs),
            other => self.install_non_path_binding(name, other),
        }
        true
    }

    /// Install a `let x = expr` binding via shallow inference on the
    /// initializer. `Path` results go into the legacy scope, non-Path
    /// results (wrappers, trait bounds) into `non_path_bindings`, and
    /// unresolvable initializers (`let s = external()` where we can't
    /// name the return type) into `non_path_bindings` as an `Opaque`
    /// tombstone so an outer `s: Session` doesn't leak back in when
    /// `s.method()` is resolved. Only simple `Pat::Ident` patterns are
    /// handled here; destructuring flows through `install_destructure_bindings`.
    /// Operation.
    pub(super) fn install_inferred_let_binding(&mut self, local: &syn::Local) {
        let Some(name) = extract_pat_ident_name(&local.pat) else {
            return;
        };
        let inferred = local
            .init
            .as_ref()
            .and_then(|init| self.infer_receiver_type(&init.expr))
            .unwrap_or(CanonicalType::Opaque);
        match inferred {
            CanonicalType::Path(segs) => self.install_path_binding(name, segs),
            other => self.install_non_path_binding(name, other),
        }
    }

    /// Extract pattern bindings from `pat` against a matched-type from
    /// `matched_expr`, installing them into the current scope. Path
    /// bindings go into the legacy scope, wrapper/trait-bound bindings
    /// into `non_path_bindings`. If the matched expression is itself
    /// unresolvable, every syntactic binding in the pattern gets an
    /// `Opaque` tombstone so outer same-name bindings can't leak back
    /// in at a later `.method()` call. Used by `let`-destructuring,
    /// `if let`, `while let`, `match` arms. Integration.
    pub(super) fn install_destructure_bindings(
        &mut self,
        pat: &syn::Pat,
        matched_expr: &syn::Expr,
    ) {
        let matched = self
            .infer_receiver_type(matched_expr)
            .unwrap_or(CanonicalType::Opaque);
        let pairs = self.extract_pattern_pairs(pat, &matched, PatKind::Value);
        self.install_binding_pairs_with_tombstones(pat, pairs);
    }

    /// Extract for-loop element-type bindings from `pat` against
    /// `iter_expr` (the thing being iterated over). Unresolvable
    /// iterators tombstone their pattern idents, same as
    /// `install_destructure_bindings`. Integration.
    pub(super) fn install_for_bindings(&mut self, pat: &syn::Pat, iter_expr: &syn::Expr) {
        let iter_type = self
            .infer_receiver_type(iter_expr)
            .unwrap_or(CanonicalType::Opaque);
        let pairs = self.extract_pattern_pairs(pat, &iter_type, PatKind::Iterator);
        self.install_binding_pairs_with_tombstones(pat, pairs);
    }

    /// Wrapper around `patterns::extract_bindings` / `extract_for_bindings`
    /// that builds a fresh `InferContext`. Operation.
    pub(super) fn extract_pattern_pairs(
        &self,
        pat: &syn::Pat,
        matched: &CanonicalType,
        kind: PatKind,
    ) -> Vec<(String, CanonicalType)> {
        let Some(workspace) = self.workspace_index else {
            return Vec::new();
        };
        let adapter = CollectorBindings {
            scope: &self.bindings,
            non_path_scope: &self.non_path_bindings,
        };
        let ictx = InferContext {
            file: self.file,
            mod_stack: self.mod_stack,
            workspace,
            bindings: &adapter,
            self_type: self.self_type_canonical.clone(),
            workspace_files: self.workspace_files,
            generic_params: Some(&self.generic_params),
            reexports: self.reexports,
        };
        match kind {
            PatKind::Value => extract_bindings(pat, matched, &ictx),
            PatKind::Iterator => extract_for_bindings(pat, matched, &ictx),
        }
    }

    /// Dispatch each `(name, type)` pair into the right scope map, then
    /// walk `pat` and install `Opaque` tombstones for every syntactic
    /// ident the resolver didn't reach. This keeps an unresolvable
    /// `let (_, s) = external()` or `for s in opaque_iter` from letting
    /// an outer `s: Session` leak back in at `s.method()` time.
    /// Operation.
    pub(super) fn install_binding_pairs_with_tombstones(
        &mut self,
        pat: &syn::Pat,
        pairs: Vec<(String, CanonicalType)>,
    ) {
        let mut resolved: HashSet<String> = HashSet::new();
        for (name, ty) in pairs {
            resolved.insert(name.clone());
            match ty {
                CanonicalType::Path(segs) => self.install_path_binding(name, segs),
                other => self.install_non_path_binding(name, other),
            }
        }
        let mut idents = Vec::new();
        collect_pattern_idents(pat, &mut idents);
        for name in idents {
            if !resolved.contains(&name) {
                self.install_non_path_binding(name, CanonicalType::Opaque);
            }
        }
    }
}

/// Whether `extract_pattern_pairs` should use value-pattern
/// (`let` / `if let` / `match`) or for-loop element-type extraction.
pub(super) enum PatKind {
    Value,
    Iterator,
}
