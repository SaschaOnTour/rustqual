//! Canonical call-target collection with receiver-type tracking.
//!
//! Turns a `syn::Block` into a `HashSet<String>` of canonical call
//! targets. Handles:
//! - `crate::` / `self::` / `super::` prefixed calls (resolved via
//!   `forbidden_rule::resolve_to_crate_absolute`).
//! - `Self::method(...)` in impl blocks (via `self_type` context).
//! - Alias-resolved unqualified calls (via `gather_alias_map`).
//! - Macro descent (`assert!(foo(x))` records `foo`).
//! - Receiver-type-tracked method calls: `let s = RlmSession::open();
//!   s.search(x);` → `crate::…::RlmSession::search` (not `<method>:search`).
//!
//! Binding-extraction helpers live in [`super::bindings`]; this file
//! owns the visitor, the scope stack, and the target canonicalisation.
//!
//! See `D-3` and `D-4` in the v1.1.0 plan for the resolution order and
//! the binding scan patterns.

use super::bindings::{
    canonical_from_type, extract_let_binding, normalize_alias_expansion, CanonScope,
};
use super::local_symbols::{scope_for_local, FileScope};
use super::type_infer::resolve::{resolve_type, ResolveContext};
use super::type_infer::self_subst::substitute_bare_self;
use super::type_infer::{
    extract_bindings, extract_for_bindings, infer_type, BindingLookup, CanonicalType, InferContext,
    WorkspaceTypeIndex,
};
use crate::adapters::analyzers::architecture::forbidden_rule::{
    file_to_module_segments, resolve_to_crate_absolute_in,
};
use crate::adapters::shared::use_tree::AliasTarget;
use std::collections::{HashMap, HashSet};
use syn::visit::Visit;

/// Canonical marker for method calls whose receiver-type we can't resolve.
/// Any `<method>:<name>` string is layer-unknown by construction and
/// never counts as a delegation target.
const METHOD_UNKNOWN_PREFIX: &str = "<method>:";
/// Canonical marker for unqualified / unresolved call paths. All `<bare>:…`
/// strings are layer-unknown (external, stdlib, or not aliased).
const BARE_UNKNOWN_PREFIX: &str = "<bare>:";

/// Input for the canonical-call collector. Per-file lookup tables live
/// in `file`; the rest is per-fn.
pub struct FnContext<'a> {
    pub file: &'a FileScope<'a>,
    /// Mod-path of the fn declaration inside `file.path`. Empty for
    /// top-level fns.
    pub mod_stack: &'a [String],
    /// Body of the function we analyse.
    pub body: &'a syn::Block,
    /// Named signature parameters with their declared types.
    pub signature_params: Vec<(String, &'a syn::Type)>,
    /// Canonical generic-param map: `name → ParamInfo` (canonicalised
    /// bounds + turbofish substitution position). Callers MUST build
    /// this via `signature_params::item_canonical_generics` or
    /// `method_canonical_generics`; constructing it ad-hoc bypasses
    /// the canonicaliser AND the position-tagging and leaves
    /// `Q::method()` dispatch / turbofish substitution broken.
    pub generic_params: HashMap<String, super::signature_params::ParamInfo>,
    /// Type-path of the enclosing `impl` block, if any.
    pub self_type: Option<Vec<String>>,
    /// Workspace type-index for shallow inference fallback. `None` for
    /// unit-test fixtures.
    pub workspace_index: Option<&'a WorkspaceTypeIndex>,
    /// All workspace `FileScope`s. Lets alias expansion switch into
    /// the alias's declaring scope. `None` for unit-test fixtures.
    pub workspace_files: Option<&'a HashMap<String, FileScope<'a>>>,
    /// Workspace-wide `pub use` re-export map. `Some(&…)` enables the
    /// gate's reexport-substitution step at every `canonicalise_workspace_path`
    /// invocation inside the body walk. `None` for legacy / unit-test
    /// fixtures.
    pub reexports: Option<&'a super::reexports::ReexportMap>,
}

// qual:api
/// Collect the canonical call-target set from a fn body. Entry point for
/// Check A / Check B call-graph construction.
pub fn collect_canonical_calls(ctx: &FnContext<'_>) -> HashSet<String> {
    let mut collector = CanonicalCallCollector::new(ctx);
    collector.seed_signature_bindings();
    collector.visit_block(ctx.body);
    collector.calls
}

struct CanonicalCallCollector<'a> {
    file: &'a FileScope<'a>,
    /// Mod-path inside `file.path` of the fn under analysis. Read-only
    /// for the duration of the body walk.
    mod_stack: &'a [String],
    /// Full canonical path of the enclosing impl's self-type (with
    /// `crate` prefix), if any — used to resolve `Self::method`.
    self_type_canonical: Option<Vec<String>>,
    signature_params: Vec<(String, &'a syn::Type)>,
    /// Generic type-param name → canonicalised trait-bound paths.
    /// Empty bounds keep the param-name reservation so an unbound
    /// `Q::method(...)` doesn't fall through to `crate_root_modules`.
    generic_params: HashMap<String, super::signature_params::ParamInfo>,
    /// Scope stack of variable-name → canonical-type-path bindings.
    /// Always non-empty while a collection is in flight.
    bindings: Vec<HashMap<String, Vec<String>>>,
    /// Parallel scope stack for non-Path bindings (`Result<…>`,
    /// `dyn Trait`, etc.). Pushed/popped in lockstep with `bindings`.
    non_path_bindings: Vec<HashMap<String, CanonicalType>>,
    calls: HashSet<String>,
    /// Workspace type-index for shallow inference fallback. `None`
    /// for unit-test fixtures.
    workspace_index: Option<&'a WorkspaceTypeIndex>,
    /// Workspace `FileScope` map for alias decl-site resolution.
    workspace_files: Option<&'a HashMap<String, FileScope<'a>>>,
    /// Workspace-wide `pub use` re-export map. Threaded into every
    /// `CanonScope` / `ResolveContext` / `InferContext` built during
    /// the body walk so the gate substitutes re-exported prefixes.
    reexports: Option<&'a super::reexports::ReexportMap>,
}

mod canon;
mod install;
mod resolve;
mod scope;
mod visit;

/// Best-effort extraction of expressions from a macro token stream.
/// Thin alias for the shared [`crate::adapters::shared::macro_tokens::recover_exprs`]
/// — the single source of the comma-list / `;`-repeat / block-bodied / single-expr
/// recovery strategy used by every macro-aware collector. Kept as a local name
/// so the call_parity call-graph code reads in its own vocabulary.
/// Integration: delegates to the shared helper, no logic.
pub(super) fn parse_macro_tokens(tokens: proc_macro2::TokenStream) -> Vec<syn::Expr> {
    crate::adapters::shared::macro_tokens::recover_exprs(&tokens)
}

/// Project an inferred receiver type to the canonical call-graph
/// edge(s) for a method call. `Path` yields one concrete edge.
/// `TraitBound` (Stage 2) yields one synthetic anchor edge
/// `<Trait>::<method>` provided the method is declared on the trait —
/// the touchpoint walker decides target-boundary status via
/// `is_anchor_target_capability` (target-declared callable body OR
/// overriding impl in target), so call-parity stays sound for
/// Ports&Adapters architectures without fanning out N per-impl edges
/// (which would otherwise turn one boundary call into N
/// false-positive Check C touchpoints). Wrapper variants
/// (`Result`/`Option`/…) yield no direct edge — the combinator table
/// already unwrapped them in the method-return lookup.
/// Adapter exposing the collector's scope stack as a `BindingLookup`.
struct CollectorBindings<'a> {
    scope: &'a [HashMap<String, Vec<String>>],
    non_path_scope: &'a [HashMap<String, CanonicalType>],
}

impl BindingLookup for CollectorBindings<'_> {
    fn lookup(&self, ident: &str) -> Option<CanonicalType> {
        // Walk both stacks in lockstep from innermost to outermost so
        // shadowing works across kinds (a wrapper-typed `let` hides an
        // outer path-typed `let` with the same name and vice versa).
        // Install helpers evict the sibling entry at the same level, so
        // at most one map hits per frame.
        for (path_frame, non_path_frame) in self
            .scope
            .iter()
            .rev()
            .zip(self.non_path_scope.iter().rev())
        {
            if let Some(ty) = non_path_frame.get(ident) {
                return Some(ty.clone());
            }
            if let Some(segs) = path_frame.get(ident) {
                return Some(CanonicalType::Path(segs.clone()));
            }
        }
        None
    }
}

/// Peel `Pat::Type` wrappers to reach a `Pat::Ident` and return its
/// identifier. Returns `None` for destructuring / tuple / struct
/// patterns — those flow through `patterns::extract_bindings`.
/// Operation: recursive pattern peel.
// qual:recursive
pub(super) fn extract_pat_ident_name(pat: &syn::Pat) -> Option<String> {
    match pat {
        syn::Pat::Ident(pi) => Some(pi.ident.to_string()),
        syn::Pat::Type(pt) => extract_pat_ident_name(&pt.pat),
        _ => None,
    }
}

/// Collect every binding ident introduced by `pat` (ignoring subpatterns
/// that don't bind names — `_`, literals, ref subslices without idents).
/// Used to install `Opaque` tombstones for syntactic bindings whose
/// matched type couldn't be inferred. Integration: dispatch over pat
/// variants, each arm delegates to a recursive helper.
// qual:recursive
pub(super) fn collect_pattern_idents(pat: &syn::Pat, out: &mut Vec<String>) {
    match pat {
        syn::Pat::Ident(pi) => push_pat_ident(pi, out),
        syn::Pat::Type(pt) => collect_pattern_idents(&pt.pat, out),
        syn::Pat::Reference(r) => collect_pattern_idents(&r.pat, out),
        syn::Pat::Paren(p) => collect_pattern_idents(&p.pat, out),
        syn::Pat::Tuple(t) => walk_each(t.elems.iter(), out),
        syn::Pat::TupleStruct(ts) => walk_each(ts.elems.iter(), out),
        syn::Pat::Struct(s) => walk_each(s.fields.iter().map(|f| f.pat.as_ref()), out),
        syn::Pat::Slice(s) => walk_each(s.elems.iter(), out),
        syn::Pat::Or(o) => walk_each(o.cases.iter().take(1), out),
        _ => {}
    }
}

/// Recurse into every pattern in `iter`. Operation: closure-free fn
/// keeps lifetime inference simple when called from the main walker.
fn walk_each<'p, I: Iterator<Item = &'p syn::Pat>>(iter: I, out: &mut Vec<String>) {
    for p in iter {
        collect_pattern_idents(p, out);
    }
}

/// Push a `Pat::Ident`'s name and recurse into its optional subpattern
/// (`x @ Some(inner)`). Operation: closure-hidden recursion.
fn push_pat_ident(pi: &syn::PatIdent, out: &mut Vec<String>) {
    out.push(pi.ident.to_string());
    if let Some((_, sub)) = &pi.subpat {
        collect_pattern_idents(sub, out);
    }
}

/// Prefix an unresolved single-ident or segment path with the layer-unknown
/// `<bare>:` marker. Centralised so the BP-010 format-repetition detector
/// sees exactly one format string, and so the marker can evolve together.
fn bare(path: &str) -> String {
    format!("{BARE_UNKNOWN_PREFIX}{path}")
}

/// Prefix a method identifier with the layer-unknown `<method>:` marker.
fn method_unknown(method: &str) -> String {
    format!("{METHOD_UNKNOWN_PREFIX}{method}")
}

// The Visit impl uses an independent `'ast` lifetime so the same
// collector can walk both the main fn body (long-lived) and macro
// bodies we parse on-the-fly (locally-owned, short-lived). The struct's
// `'a` carries state references (alias_map etc.); it never constrains
