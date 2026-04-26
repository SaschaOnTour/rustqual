//! Shallow type-inference engine.
//!
//! Public API: `infer_type(expr, ctx) -> Option<CanonicalType>`.
//!
//! Dispatches over `syn::Expr` variants (see
//! `docs/rustqual-design-receiver-type-inference.md` §3). Each variant
//! delegates to `call` or `access` modules; transparent wrappers
//! (`Paren`, `Reference`, `Group`) recurse directly. Stdlib
//! Result/Option/Future combinators (`.unwrap()`, `.map_err()`, `.await`,
//! …) are handled via the `combinators` table.
//!
//! The engine consumes bindings through the `BindingLookup` trait —
//! callers (the call-graph collector, pattern-binding helpers) are
//! responsible for populating the scope before delegating here.

pub mod access;
pub mod call;
pub mod generics;

use super::super::local_symbols::FileScope;
use super::canonical::CanonicalType;
use super::workspace_index::WorkspaceTypeIndex;
use std::collections::HashMap;

/// Look up a scoped variable name → inferred type. Implementations may
/// back this by a flat map (tests), a stack of maps, or an adapter over
/// the collector's existing scope. Returns an owned value so adapters
/// can synthesize `CanonicalType`s on the fly without lifetime
/// gymnastics.
pub trait BindingLookup {
    fn lookup(&self, ident: &str) -> Option<CanonicalType>;
}

/// Simple flat-map `BindingLookup` impl. Used by unit tests and as a
/// starting point for downstream consumers who don't need scoped
/// push/pop semantics.
#[derive(Debug, Default)]
pub struct FlatBindings {
    map: HashMap<String, CanonicalType>,
}

impl FlatBindings {
    // qual:api
    pub fn new() -> Self {
        Self::default()
    }

    // qual:api
    /// Record a binding. Replaces an existing entry for the same name.
    /// Operation.
    pub fn insert(&mut self, name: &str, ty: CanonicalType) {
        self.map.insert(name.to_string(), ty);
    }
}

impl BindingLookup for FlatBindings {
    fn lookup(&self, ident: &str) -> Option<CanonicalType> {
        self.map.get(ident).cloned()
    }
}

/// Inputs to the inference engine. Per-file lookup tables live in
/// `file`; the rest is per-call-site.
pub struct InferContext<'a> {
    pub file: &'a FileScope<'a>,
    /// Mod-path of the call site inside `file.path`. Empty for
    /// top-level inference.
    pub mod_stack: &'a [String],
    pub workspace: &'a WorkspaceTypeIndex,
    pub bindings: &'a dyn BindingLookup,
    /// Canonical segments of the enclosing `impl T { ... }`'s self-type,
    /// if we're currently inferring inside an impl body. `None` for
    /// free-fn contexts.
    pub self_type: Option<Vec<String>>,
    /// All workspace `FileScope`s, for cross-module alias resolution.
    /// `None` for unit-test fixtures.
    pub workspace_files: Option<&'a HashMap<String, FileScope<'a>>>,
}

// qual:api
/// Infer the canonical type of a `syn::Expr`. Integration: dispatches
/// over expression variants to the `call` / `access` sub-modules.
/// Returns `None` when the expression shape isn't supported or when
/// inference inputs are insufficient to pin down a concrete type.
// qual:recursive
pub fn infer_type(expr: &syn::Expr, ctx: &InferContext<'_>) -> Option<CanonicalType> {
    match expr {
        syn::Expr::Path(p) => call::infer_path_expr(p, ctx),
        syn::Expr::Call(c) => call::infer_call(c, ctx),
        syn::Expr::MethodCall(m) => call::infer_method_call(m, ctx),
        syn::Expr::Field(f) => access::infer_field(f, ctx),
        syn::Expr::Try(t) => access::infer_try(t, ctx),
        syn::Expr::Await(a) => access::infer_await(a, ctx),
        syn::Expr::Paren(p) => infer_type(&p.expr, ctx),
        syn::Expr::Reference(r) => infer_type(&r.expr, ctx),
        syn::Expr::Group(g) => infer_type(&g.expr, ctx),
        syn::Expr::Cast(c) => access::infer_cast(c, ctx),
        syn::Expr::Unary(u) => access::infer_unary(u, ctx),
        _ => None,
    }
}
