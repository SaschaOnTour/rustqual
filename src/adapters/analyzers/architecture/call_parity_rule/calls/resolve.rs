//! Method-call target resolution + trait/edge projection.

use super::super::type_infer::{infer_type, CanonicalType, InferContext, WorkspaceTypeIndex};
use super::{CanonicalCallCollector, CollectorBindings};

impl<'a> CanonicalCallCollector<'a> {
    pub(super) fn record_call(&mut self, target: String) {
        self.calls.insert(target);
    }

    /// Resolve a method call's receiver to the canonical call-graph
    /// targets. Fast-path returns a single element; trait-dispatch
    /// inference returns the synthetic anchor `<Trait>::<method>` so
    /// `dyn Trait` calls collapse to one boundary regardless of how
    /// many impls exist. Empty vec means unresolved — caller records
    /// `<method>:name`. Integration: fast-path first, inference
    /// fallback second.
    pub(super) fn resolve_method_targets(
        &self,
        receiver: &syn::Expr,
        method_name: &str,
    ) -> Vec<String> {
        if let Some(c) = self.try_fast_path_receiver(receiver, method_name) {
            return vec![c];
        }
        self.try_inferred_targets(receiver, method_name)
    }

    /// Fast-path: receiver is a bare ident with a concrete binding in
    /// the legacy path scope. Walks both scope stacks from innermost to
    /// outermost so a non-path shadow (`let r: Result<_,_> = …`
    /// shadowing an outer `let r: Session = …`) aborts the fast-path
    /// and hands off to inference, instead of producing a stale concrete
    /// edge. Operation.
    pub(super) fn try_fast_path_receiver(
        &self,
        receiver: &syn::Expr,
        method_name: &str,
    ) -> Option<String> {
        let syn::Expr::Path(p) = receiver else {
            return None;
        };
        if p.path.segments.len() != 1 {
            return None;
        }
        let ident = p.path.segments[0].ident.to_string();
        for (path_scope, non_path_scope) in self
            .bindings
            .iter()
            .rev()
            .zip(self.non_path_bindings.iter().rev())
        {
            if non_path_scope.contains_key(&ident) {
                return None;
            }
            if let Some(binding) = path_scope.get(&ident) {
                let mut full = binding.clone();
                full.push(method_name.to_string());
                return Some(full.join("::"));
            }
        }
        None
    }

    /// Inference fallback: run shallow type inference over the receiver
    /// expression, then project the result into one or more canonical
    /// call-graph targets. Returns `Vec::new()` when the workspace index
    /// isn't present, inference fails, or the inferred type isn't
    /// resolvable to a concrete edge. Operation.
    pub(super) fn try_inferred_targets(
        &self,
        receiver: &syn::Expr,
        method_name: &str,
    ) -> Vec<String> {
        let Some(workspace) = self.workspace_index else {
            return Vec::new();
        };
        let Some(inferred) = self.infer_receiver_type(receiver) else {
            return Vec::new();
        };
        canonical_edges_for_method(&inferred, method_name, workspace)
    }

    /// Run `infer_type` over `receiver` with the current collector
    /// state. Returns the raw `CanonicalType` so `try_inferred_targets`
    /// can project it to 0/1/N edges. Operation: adapter build +
    /// delegate.
    pub(super) fn infer_receiver_type(&self, expr: &syn::Expr) -> Option<CanonicalType> {
        let adapter = CollectorBindings {
            scope: &self.bindings,
            non_path_scope: &self.non_path_bindings,
        };
        let ctx = InferContext {
            file: self.file,
            mod_stack: self.mod_stack,
            workspace: self.workspace_index?,
            bindings: &adapter,
            self_type: self.self_type_canonical.clone(),
            workspace_files: self.workspace_files,
            generic_params: Some(&self.generic_params),
            reexports: self.reexports,
        };
        infer_type(expr, &ctx)
    }
}

pub(super) fn canonical_edges_for_method(
    ty: &CanonicalType,
    method: &str,
    workspace: &WorkspaceTypeIndex,
) -> Vec<String> {
    // Both TraitBound (impl/dyn Trait) and GenericParamBound
    // (`fn f<Q: T>() -> Q`) dispatch identically through the trait
    // anchor — only their turbofish-overridability differs. Use
    // `as_trait_bounds()` so both variants route together.
    if let Some(bounds) = ty.as_trait_bounds() {
        return bounds
            .iter()
            .flat_map(|trait_segs| trait_dispatch_edges(trait_segs, method, workspace))
            .collect();
    }
    match ty {
        CanonicalType::Path(segs) => {
            let mut full = segs.clone();
            full.push(method.to_string());
            vec![full.join("::")]
        }
        _ => Vec::new(),
    }
}

/// Emit a single synthetic trait-method anchor `<Trait>::<method>` for
/// `dyn Trait.method()` dispatch. The anchor represents the logical
/// capability; concrete impls are NOT fanned out as separate edges
/// here — fanout would build N-element touchpoint sets that fire
/// Check C false-positives for a single boundary call. The anchor is
/// intentionally treated as a leaf in the call graph (no
/// anchor → impl edges are added) — calls inside default bodies and
/// overriding impl bodies are out of scope; documented as a known
/// limitation in `book/adapter-parity.md`. Filters on
/// `trait_has_method` so `dyn Trait.unrelated_method()` still falls
/// through to `<method>:name`. Operation: index lookup.
pub(super) fn trait_dispatch_edges(
    trait_segs: &[String],
    method: &str,
    workspace: &WorkspaceTypeIndex,
) -> Vec<String> {
    let trait_canonical = trait_segs.join("::");
    if !workspace.trait_has_method(&trait_canonical, method) {
        return Vec::new();
    }
    vec![format!("{trait_canonical}::{method}")]
}
