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

use super::bindings::{canonical_from_type, extract_let_binding, normalize_alias_expansion};
use super::local_symbols::scope_for_local;
use super::type_infer::resolve::{resolve_type, ResolveContext};
use super::type_infer::{
    extract_bindings, extract_for_bindings, infer_type, BindingLookup, CanonicalType, InferContext,
    WorkspaceTypeIndex,
};
use crate::adapters::analyzers::architecture::forbidden_rule::{
    file_to_module_segments, resolve_to_crate_absolute_in,
};
use std::collections::{HashMap, HashSet};
use syn::visit::Visit;

/// Canonical marker for method calls whose receiver-type we can't resolve.
/// Any `<method>:<name>` string is layer-unknown by construction and
/// never counts as a delegation target.
const METHOD_UNKNOWN_PREFIX: &str = "<method>:";
/// Canonical marker for unqualified / unresolved call paths. All `<bare>:…`
/// strings are layer-unknown (external, stdlib, or not aliased).
const BARE_UNKNOWN_PREFIX: &str = "<bare>:";

/// Input for the canonical-call collector. Bundles the fn body with its
/// signature types, impl-self-type context, and file-level alias map.
pub struct FnContext<'a> {
    /// Body of the function we analyse.
    pub body: &'a syn::Block,
    /// Named signature parameters with their declared types. Feeds the
    /// top-level binding scope so `fn foo(s: Session) { s.search() }`
    /// resolves correctly.
    pub signature_params: Vec<(String, &'a syn::Type)>,
    /// Type-path of the enclosing `impl` block, if any. Just the
    /// type-name segments (e.g. `["RlmSession"]`), or a crate-rooted
    /// path like `["crate","foo","Bar"]` for `impl crate::foo::Bar`.
    pub self_type: Option<Vec<String>>,
    /// File-level import alias map (output of `gather_alias_map`).
    pub alias_map: &'a HashMap<String, Vec<String>>,
    /// Set of top-level + nested item names declared in the same file.
    /// Unqualified calls (`helper()`, no `use` statement) whose first
    /// segment is in this set resolve to `crate::<file_module>::<...>`.
    pub local_symbols: &'a HashSet<String>,
    /// Set of crate-root module names (first-segment `<name>` for every
    /// `src/<name>.rs` / `src/<name>/**.rs` in the workspace). Lets the
    /// Rust 2018+ absolute-import form `use app::foo;` resolve to
    /// `crate::app::foo` instead of a dead-end `app::foo` canonical.
    pub crate_root_modules: &'a HashSet<String>,
    /// File path of the fn under analysis. Used to resolve
    /// `crate::` / `self::` / `super::` prefixes and `Self::…`.
    pub importing_file: &'a str,
    /// Workspace type-index for shallow inference fallback. `None` means
    /// the collector falls back to `<method>:name` for complex receivers
    /// (typical in unit-test fixtures that don't build the full graph).
    /// The full `build_call_graph` pipeline always passes `Some(&index)`.
    pub workspace_index: Option<&'a WorkspaceTypeIndex>,
    /// Mod-path of the fn declaration inside `importing_file`. Empty
    /// for top-level fns. Used together with `local_decl_scopes` so a
    /// fn `crate::file::inner::make` references its sibling type
    /// `Session` and resolves to `crate::file::inner::Session`.
    pub mod_stack: &'a [String],
    /// Per-name list of declaring mod-paths within `importing_file`.
    /// `None` for legacy / test callers — the resolver falls back to
    /// flat top-level prepend behaviour.
    pub local_decl_scopes: Option<&'a HashMap<String, Vec<Vec<String>>>>,
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

// qual:allow(srp) — LCOM4 here counts visitor methods (touch `calls`)
// separately from scope helpers (touch `bindings`). They're the two
// halves of a single walk; splitting them further fragments the
// visit-order invariants the walker depends on.
struct CanonicalCallCollector<'a> {
    alias_map: &'a HashMap<String, Vec<String>>,
    local_symbols: &'a HashSet<String>,
    crate_root_modules: &'a HashSet<String>,
    importing_file: &'a str,
    /// Full canonical path of the enclosing impl's self-type (with
    /// `crate` prefix), if any — used to resolve `Self::method`.
    self_type_canonical: Option<Vec<String>>,
    signature_params: Vec<(String, &'a syn::Type)>,
    /// Mod-path inside `importing_file` of the fn under analysis.
    /// Borrowed from `FnContext.mod_stack` — Rust doesn't let inner
    /// `mod` items shadow outer resolution, so the path is read-only
    /// for the duration of the body walk.
    mod_stack: &'a [String],
    /// Per-name declaring mod-paths within `importing_file`. Mirrors
    /// `FnContext.local_decl_scopes` and feeds the scope-aware
    /// `canonicalise_path` branch.
    local_decl_scopes: Option<&'a HashMap<String, Vec<Vec<String>>>>,
    /// Scope stack of variable-name → canonical-type-path bindings.
    /// Inner-most scope is at the end; lookup walks from back to front.
    /// Always non-empty while a collection is in flight.
    bindings: Vec<HashMap<String, Vec<String>>>,
    /// Parallel scope stack for bindings whose inferred type isn't a
    /// simple `Path` — trait bounds (`dyn Trait`) and stdlib wrappers
    /// (`Result<T, _>`, `Option<T>`, `Future<T>`, `Vec<T>`,
    /// `HashMap<_, V>`). Pushed/popped in lockstep with `bindings` so
    /// non-path bindings respect lexical scope just like path ones.
    /// Kept parallel (not merged into a single `CanonicalType` stack)
    /// because the legacy fast-path reads from `bindings` by segment
    /// vector directly — migrating that is a separate refactor.
    non_path_bindings: Vec<HashMap<String, CanonicalType>>,
    calls: HashSet<String>,
    /// Workspace type-index for shallow inference fallback. Mirrored
    /// from `FnContext` so the visitor doesn't need the full context
    /// passed through every method.
    workspace_index: Option<&'a WorkspaceTypeIndex>,
}

impl<'a> CanonicalCallCollector<'a> {
    fn new(ctx: &'a FnContext<'a>) -> Self {
        let self_type_canonical = ctx.self_type.as_ref().map(|segs| {
            // Qualified impl path (`impl crate::foo::Bar { ... }`) — use
            // as-is so Self::method canonicalises to `crate::foo::Bar::method`,
            // matching graph nodes built via `canonical_fn_name`.
            if segs.first().map(|s| s.as_str()) == Some("crate") {
                return segs.clone();
            }
            // Insert `mod_stack` between file segments and impl segments so
            // an `impl Session { ... }` declared inside `mod inner` resolves
            // `Self::method` to `crate::<file>::inner::Session::method` —
            // matching the path the graph node + type-index keys use.
            let mut full = vec!["crate".to_string()];
            full.extend(file_to_module_segments(ctx.importing_file));
            full.extend(ctx.mod_stack.iter().cloned());
            full.extend_from_slice(segs);
            full
        });
        Self {
            alias_map: ctx.alias_map,
            local_symbols: ctx.local_symbols,
            crate_root_modules: ctx.crate_root_modules,
            importing_file: ctx.importing_file,
            self_type_canonical,
            signature_params: ctx.signature_params.clone(),
            mod_stack: ctx.mod_stack,
            local_decl_scopes: ctx.local_decl_scopes,
            bindings: vec![HashMap::new()],
            non_path_bindings: vec![HashMap::new()],
            calls: HashSet::new(),
            workspace_index: ctx.workspace_index,
        }
    }

    fn seed_signature_bindings(&mut self) {
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
                self.alias_map,
                self.local_symbols,
                self.crate_root_modules,
                self.importing_file,
            ) {
                self.bindings[0].insert(name.clone(), canonical);
            }
        }
    }

    /// Install a signature-param binding using the full `resolve_type`
    /// pipeline. `Path` → legacy `Vec<String>` scope (simple and cheap
    /// to look up). `Opaque` is dropped. Everything else — `TraitBound`,
    /// `Result`/`Option`/`Future` wrappers, `Slice`/`Map` — lands in
    /// `non_path_bindings` so `?` / `.await` / trait-dispatch fire
    /// correctly on method-call sites. Operation.
    fn seed_param_via_resolver(&mut self, name: &str, ty: &syn::Type) {
        let rctx = ResolveContext {
            alias_map: self.alias_map,
            local_symbols: self.local_symbols,
            crate_root_modules: self.crate_root_modules,
            importing_file: self.importing_file,
            type_aliases: self.workspace_index.map(|w| &w.type_aliases),
            transparent_wrappers: self.workspace_index.map(|w| &w.transparent_wrappers),
            local_decl_scopes: self.local_decl_scopes,
            mod_stack: self.mod_stack,
        };
        match resolve_type(ty, &rctx) {
            CanonicalType::Path(segs) => {
                self.bindings[0].insert(name.to_string(), segs);
            }
            CanonicalType::Opaque => {}
            other => {
                // Signature params always seed the outermost scope (frame 0).
                self.non_path_bindings[0].insert(name.to_string(), other);
            }
        }
    }

    fn enter_scope(&mut self) {
        self.bindings.push(HashMap::new());
        self.non_path_bindings.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.bindings.pop();
        self.non_path_bindings.pop();
    }

    /// Return the innermost binding scope. The stack is seeded non-empty
    /// in `new()` and only mutated via paired `enter_scope` / `exit_scope`
    /// calls, so `last_mut()` is always `Some`; fall back to index access
    /// to avoid panic-helper methods in production code.
    fn current_scope_mut(&mut self) -> &mut HashMap<String, Vec<String>> {
        if self.bindings.is_empty() {
            self.bindings.push(HashMap::new());
        }
        let last = self.bindings.len() - 1;
        &mut self.bindings[last]
    }

    /// Parallel accessor for the non-path scope stack. Same invariants
    /// and fallback semantics as `current_scope_mut`.
    fn current_non_path_scope_mut(&mut self) -> &mut HashMap<String, CanonicalType> {
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
    fn install_path_binding(&mut self, name: String, segs: Vec<String>) {
        self.current_non_path_scope_mut().remove(&name);
        self.current_scope_mut().insert(name, segs);
    }

    /// Install a wrapper / trait-bound binding in the non-path scope
    /// and evict any stale Path binding for the same name (shadowing
    /// the other way). Operation.
    fn install_non_path_binding(&mut self, name: String, ty: CanonicalType) {
        self.current_scope_mut().remove(&name);
        self.current_non_path_scope_mut().insert(name, ty);
    }

    /// Turn a path-segment list into the canonical String used for all
    /// call-target comparisons in the call-parity check. Integration:
    /// each branch delegates to a dedicated helper.
    fn canonicalise_path(&self, segments: &[String]) -> String {
        if segments.is_empty() {
            return String::new();
        }
        if segments[0] == "Self" {
            return self.canonicalise_self_path(segments);
        }
        if matches!(segments[0].as_str(), "crate" | "self" | "super") {
            return self.canonicalise_keyword_path(segments);
        }
        if let Some(canonical) = self.canonicalise_alias_path(segments) {
            return canonical;
        }
        if self.local_symbols.contains(&segments[0]) {
            return self.canonicalise_local_symbol_path(segments);
        }
        // Rust 2018+ absolute call: `app::foo()` without `use` is the
        // crate-root `app` module, equivalent to `crate::app::foo()`.
        // If `app` is a known workspace root module, prepend `crate::`
        // so the canonical matches graph nodes.
        if self.crate_root_modules.contains(&segments[0]) {
            let mut full = vec!["crate".to_string()];
            full.extend_from_slice(segments);
            return full.join("::");
        }
        // Unknown path (external crate, stdlib, or not imported) → bare.
        bare(&segments.join("::"))
    }

    /// `Self::method` — substitute the enclosing impl's canonical
    /// self-type for `Self`. Falls back to `<bare>:` when we're not
    /// inside an impl. Operation.
    fn canonicalise_self_path(&self, segments: &[String]) -> String {
        if let Some(self_canonical) = &self.self_type_canonical {
            let mut full = self_canonical.clone();
            full.extend_from_slice(&segments[1..]);
            return full.join("::");
        }
        bare(&segments.join("::"))
    }

    fn canonicalise_keyword_path(&self, segments: &[String]) -> String {
        if let Some(resolved) =
            resolve_to_crate_absolute_in(self.importing_file, self.mod_stack, segments)
        {
            let mut full = vec!["crate".to_string()];
            full.extend(resolved);
            return full.join("::");
        }
        bare(&segments.join("::"))
    }

    /// First segment hits the file's import-alias map → replace the
    /// prefix and re-normalise (alias may itself reference `self::` or
    /// a Rust-2018 crate-root module). Returns `None` when no alias
    /// matches. Operation.
    fn canonicalise_alias_path(&self, segments: &[String]) -> Option<String> {
        let alias = self.alias_map.get(&segments[0])?;
        let mut full = alias.clone();
        full.extend_from_slice(&segments[1..]);
        let normalized =
            normalize_alias_expansion(full, self.importing_file, self.crate_root_modules)?;
        Some(normalized.join("::"))
    }

    /// Same-file fallback: first segment is declared in this file, so
    /// resolve to `crate::<file_module>::<mod_path>::<segments>`. The
    /// mod-path walk picks the closest enclosing scope so a call inside
    /// `mod inner` to a sibling `helper()` reaches
    /// `crate::<file>::inner::helper`. Operation.
    fn canonicalise_local_symbol_path(&self, segments: &[String]) -> String {
        let mod_path = scope_for_local(self.local_decl_scopes, &segments[0], self.mod_stack);
        let mut full = vec!["crate".to_string()];
        full.extend(file_to_module_segments(self.importing_file));
        full.extend(mod_path.iter().cloned());
        full.extend_from_slice(segments);
        full.join("::")
    }

    fn record_call(&mut self, target: String) {
        self.calls.insert(target);
    }

    /// Resolve a method call's receiver to the canonical call-graph
    /// targets. Fast-path returns a single element; trait-dispatch
    /// inference may return multiple (one per impl of the trait).
    /// Empty vec means unresolved — caller records `<method>:name`.
    /// Integration: fast-path first, inference fallback second.
    fn resolve_method_targets(&self, receiver: &syn::Expr, method_name: &str) -> Vec<String> {
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
    fn try_fast_path_receiver(&self, receiver: &syn::Expr, method_name: &str) -> Option<String> {
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
    fn try_inferred_targets(&self, receiver: &syn::Expr, method_name: &str) -> Vec<String> {
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
    fn infer_receiver_type(&self, expr: &syn::Expr) -> Option<CanonicalType> {
        let adapter = CollectorBindings {
            scope: &self.bindings,
            non_path_scope: &self.non_path_bindings,
        };
        let ctx = InferContext {
            workspace: self.workspace_index?,
            alias_map: self.alias_map,
            local_symbols: self.local_symbols,
            crate_root_modules: self.crate_root_modules,
            importing_file: self.importing_file,
            bindings: &adapter,
            self_type: self.self_type_canonical.clone(),
            mod_stack: self.mod_stack,
            local_decl_scopes: self.local_decl_scopes,
        };
        infer_type(expr, &ctx)
    }

    /// `let x: T = …` — route the annotation through the full resolver
    /// when a workspace index is available so alias expansion + wrapper
    /// peeling + trait-bound extraction all apply. Returns `true` when
    /// a binding was installed. Returns `false` only when there's no
    /// workspace index, the pattern isn't a typed ident, or the
    /// annotation is the explicit `_` inference placeholder — the caller
    /// then falls through to initializer-based inference (which is what
    /// rustc does). An annotation that names an unresolvable type still
    /// installs an `Opaque` tombstone so outer path bindings with the
    /// same name don't leak back in. Operation.
    fn try_install_annotated_binding(&mut self, local: &syn::Local) -> bool {
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
            alias_map: self.alias_map,
            local_symbols: self.local_symbols,
            crate_root_modules: self.crate_root_modules,
            importing_file: self.importing_file,
            type_aliases: Some(&wi.type_aliases),
            transparent_wrappers: Some(&wi.transparent_wrappers),
            local_decl_scopes: self.local_decl_scopes,
            mod_stack: self.mod_stack,
        };
        let name = pi.ident.to_string();
        match resolve_type(pt.ty.as_ref(), &rctx) {
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
    fn install_inferred_let_binding(&mut self, local: &syn::Local) {
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

    fn collect_macro_body(&mut self, mac: &syn::Macro) {
        for expr in parse_macro_tokens(mac.tokens.clone()) {
            self.visit_expr(&expr);
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
    fn install_destructure_bindings(&mut self, pat: &syn::Pat, matched_expr: &syn::Expr) {
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
    fn install_for_bindings(&mut self, pat: &syn::Pat, iter_expr: &syn::Expr) {
        let iter_type = self
            .infer_receiver_type(iter_expr)
            .unwrap_or(CanonicalType::Opaque);
        let pairs = self.extract_pattern_pairs(pat, &iter_type, PatKind::Iterator);
        self.install_binding_pairs_with_tombstones(pat, pairs);
    }

    /// Wrapper around `patterns::extract_bindings` / `extract_for_bindings`
    /// that builds a fresh `InferContext`. Operation.
    fn extract_pattern_pairs(
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
            workspace,
            alias_map: self.alias_map,
            local_symbols: self.local_symbols,
            crate_root_modules: self.crate_root_modules,
            importing_file: self.importing_file,
            bindings: &adapter,
            self_type: self.self_type_canonical.clone(),
            mod_stack: self.mod_stack,
            local_decl_scopes: self.local_decl_scopes,
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
    fn install_binding_pairs_with_tombstones(
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
enum PatKind {
    Value,
    Iterator,
}

/// Best-effort extraction of expressions from a macro token stream.
/// Most macros accept comma-separated exprs (`assert!(a, b)`,
/// `format!("{}", x)`), but block-like bodies (`tokio::select! { ... }`)
/// and separator-`;` variants (`vec![x; n]`) don't. We try three
/// strategies in order:
/// 1. Comma-separated `syn::Expr` list (covers ~90% of macro calls).
/// 2. Brace-wrapped parse as a `syn::Block` — extracts every statement
///    expression, covering block-bodied and `;`-separated forms.
/// 3. Single `syn::Expr` — for macros whose argument is one expression.
///
/// Still silent-skips on total parse failure (extern-DSL macros, custom
/// grammar) — a documented limitation of syntax-level call-graph
/// construction.
fn parse_macro_tokens(tokens: proc_macro2::TokenStream) -> Vec<syn::Expr> {
    use syn::parse::Parser;
    use syn::punctuated::Punctuated;
    use syn::Token;
    let parser = Punctuated::<syn::Expr, Token![,]>::parse_terminated;
    if let Ok(exprs) = parser.parse2(tokens.clone()) {
        return exprs.into_iter().collect();
    }
    let braced = quote::quote! { { #tokens } };
    if let Ok(block) = syn::parse2::<syn::Block>(braced) {
        return block
            .stmts
            .into_iter()
            .filter_map(|stmt| match stmt {
                syn::Stmt::Expr(e, _) => Some(e),
                syn::Stmt::Local(l) => l.init.map(|init| *init.expr),
                _ => None,
            })
            .collect();
    }
    if let Ok(expr) = syn::parse2::<syn::Expr>(tokens) {
        return vec![expr];
    }
    Vec::new()
}

/// Project an inferred receiver type to the canonical call-graph
/// edge(s) for a method call. `Path` yields one edge. `TraitBound`
/// (Stage 2) yields one edge per workspace impl of the trait,
/// provided the method is declared on the trait — the over-approximation
/// that makes call-parity sound for Ports&Adapters architectures.
/// Wrapper variants (`Result`/`Option`/…) yield no direct edge — the
/// combinator table already unwrapped them in the method-return lookup.
/// Operation: variant dispatch.
fn canonical_edges_for_method(
    ty: &CanonicalType,
    method: &str,
    workspace: &WorkspaceTypeIndex,
) -> Vec<String> {
    match ty {
        CanonicalType::Path(segs) => {
            let mut full = segs.clone();
            full.push(method.to_string());
            vec![full.join("::")]
        }
        CanonicalType::TraitBound(segs) => trait_dispatch_edges(segs, method, workspace),
        _ => Vec::new(),
    }
}

/// Enumerate one edge per workspace impl of the trait. Filters on
/// `trait_has_method` so `dyn Trait.unrelated_method()` still falls
/// through to `<method>:name`. Operation: index lookup + map.
fn trait_dispatch_edges(
    trait_segs: &[String],
    method: &str,
    workspace: &WorkspaceTypeIndex,
) -> Vec<String> {
    let trait_canonical = trait_segs.join("::");
    if !workspace.trait_has_method(&trait_canonical, method) {
        return Vec::new();
    }
    workspace
        .impls_of_trait(&trait_canonical)
        .iter()
        .map(|impl_type| format!("{impl_type}::{method}"))
        .collect()
}

/// Adapter that exposes the collector's `Vec<HashMap<String, Vec<String>>>`
/// scope stack as a `BindingLookup` for the inference engine. Bindings
/// in the old scope are always concrete type paths, so we wrap each as
/// `CanonicalType::Path(segs)`. Stdlib-wrapper bindings (`Option<T>`,
/// `Result<T,_>`) are never stored in the old scope — they're either
/// unwrapped via `?` before `let` binds them, or simply not populated
/// by the legacy `extract_let_binding`.
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
fn extract_pat_ident_name(pat: &syn::Pat) -> Option<String> {
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
fn collect_pattern_idents(pat: &syn::Pat, out: &mut Vec<String>) {
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
// the AST lifetime.
impl<'a, 'ast> Visit<'ast> for CanonicalCallCollector<'a> {
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.enter_scope();
        syn::visit::visit_block(self, block);
        self.exit_scope();
    }

    fn visit_local(&mut self, local: &'ast syn::Local) {
        // Walk the initializer first so calls in the RHS are recorded
        // before the binding is installed. Rust shadowing semantics
        // reference the outer binding in the RHS.
        if let Some(init) = &local.init {
            self.visit_expr(&init.expr);
            if let Some((_, else_expr)) = &init.diverge {
                self.visit_expr(else_expr);
            }
        }
        // `let x: T = …` with a workspace index — route the annotation
        // through the full resolver so Stage-3 alias expansion + wrapper
        // peeling + trait-bound extraction apply. Without this, `let r:
        // AppResult<Session> = …` would cache the raw alias path and
        // later `r.unwrap().m()` would miss the method-return edge.
        if self.try_install_annotated_binding(local) {
            return;
        }
        // Fast-path: direct `let s = T::ctor()` — the legacy prefix-based
        // extractor resolves without needing a populated workspace index,
        // so unit-test fixtures work.
        if let Some((name, ty_canonical)) = extract_let_binding(
            local,
            self.alias_map,
            self.local_symbols,
            self.crate_root_modules,
            self.importing_file,
        ) {
            self.install_path_binding(name, ty_canonical);
            return;
        }
        // Simple-ident inference fallback (handles method chains + wrapper types).
        if extract_pat_ident_name(&local.pat).is_some() {
            self.install_inferred_let_binding(local);
            return;
        }
        // Destructuring: `let Some(x) = opt`, `let Ctx { field } = …`,
        // `let (a, b) = …`, `let Pat = expr else { return; }`. Install
        // all pattern-extracted bindings into the current scope.
        if let Some(init) = local.init.as_ref() {
            self.install_destructure_bindings(&local.pat, &init.expr);
        }
    }

    fn visit_expr_if(&mut self, expr_if: &'ast syn::ExprIf) {
        self.enter_scope();
        // `if let PAT = SCRUTINEE { THEN }` — extract bindings visible
        // in the then-block only. Non-let conditions are visited via
        // the default walker and don't introduce bindings.
        if let syn::Expr::Let(let_expr) = expr_if.cond.as_ref() {
            self.visit_expr(&let_expr.expr);
            self.install_destructure_bindings(&let_expr.pat, &let_expr.expr);
        } else {
            self.visit_expr(&expr_if.cond);
        }
        self.visit_block(&expr_if.then_branch);
        self.exit_scope();
        if let Some((_, else_branch)) = &expr_if.else_branch {
            self.visit_expr(else_branch);
        }
    }

    fn visit_expr_while(&mut self, expr_while: &'ast syn::ExprWhile) {
        self.enter_scope();
        if let syn::Expr::Let(let_expr) = expr_while.cond.as_ref() {
            self.visit_expr(&let_expr.expr);
            self.install_destructure_bindings(&let_expr.pat, &let_expr.expr);
        } else {
            self.visit_expr(&expr_while.cond);
        }
        self.visit_block(&expr_while.body);
        self.exit_scope();
    }

    fn visit_expr_match(&mut self, expr_match: &'ast syn::ExprMatch) {
        self.visit_expr(&expr_match.expr);
        for arm in &expr_match.arms {
            self.enter_scope();
            self.install_destructure_bindings(&arm.pat, &expr_match.expr);
            if let Some((_, guard)) = &arm.guard {
                self.visit_expr(guard);
            }
            self.visit_expr(&arm.body);
            self.exit_scope();
        }
    }

    fn visit_expr_for_loop(&mut self, for_loop: &'ast syn::ExprForLoop) {
        self.visit_expr(&for_loop.expr);
        self.enter_scope();
        self.install_for_bindings(&for_loop.pat, &for_loop.expr);
        self.visit_block(&for_loop.body);
        self.exit_scope();
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        // Walk func + args first so nested calls / macros are recorded.
        self.visit_expr(&call.func);
        for arg in &call.args {
            self.visit_expr(arg);
        }
        if let syn::Expr::Path(p) = call.func.as_ref() {
            let segments: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            let canonical = self.canonicalise_path(&segments);
            self.record_call(canonical);
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        // Walk receiver + args so nested resolution / method chains record.
        self.visit_expr(&call.receiver);
        for arg in &call.args {
            self.visit_expr(arg);
        }
        let method_name = call.method.to_string();
        let targets = self.resolve_method_targets(&call.receiver, &method_name);
        if targets.is_empty() {
            self.record_call(method_unknown(&method_name));
        } else {
            for t in targets {
                self.record_call(t);
            }
        }
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.collect_macro_body(mac);
    }

    fn visit_expr_closure(&mut self, c: &'ast syn::ExprClosure) {
        self.enter_scope();
        // Closure params: extract typed idents into scope.
        for input in &c.inputs {
            if let syn::Pat::Type(pt) = input {
                if let syn::Pat::Ident(pi) = pt.pat.as_ref() {
                    let name = pi.ident.to_string();
                    if let Some(canonical) = canonical_from_type(
                        &pt.ty,
                        self.alias_map,
                        self.local_symbols,
                        self.crate_root_modules,
                        self.importing_file,
                    ) {
                        self.current_scope_mut().insert(name, canonical);
                    }
                }
            }
        }
        self.visit_expr(&c.body);
        self.exit_scope();
    }
}
