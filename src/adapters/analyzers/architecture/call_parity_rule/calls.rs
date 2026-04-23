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
use crate::adapters::analyzers::architecture::forbidden_rule::{
    file_to_module_segments, resolve_to_crate_absolute,
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
    /// Set of top-level item names declared in the same file. Unqualified
    /// calls (`helper()`, no `use` statement) whose first segment is in
    /// this set resolve to `crate::<file_module>::<ident>` so the call
    /// graph sees local delegation chains.
    pub local_symbols: &'a HashSet<String>,
    /// File path of the fn under analysis. Used to resolve
    /// `crate::` / `self::` / `super::` prefixes and `Self::…`.
    pub importing_file: &'a str,
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
    importing_file: &'a str,
    /// Full canonical path of the enclosing impl's self-type (with
    /// `crate` prefix), if any — used to resolve `Self::method`.
    self_type_canonical: Option<Vec<String>>,
    signature_params: Vec<(String, &'a syn::Type)>,
    /// Scope stack of variable-name → canonical-type-path bindings.
    /// Inner-most scope is at the end; lookup walks from back to front.
    /// Always non-empty while a collection is in flight.
    bindings: Vec<HashMap<String, Vec<String>>>,
    calls: HashSet<String>,
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
            let mut full = vec!["crate".to_string()];
            full.extend(file_to_module_segments(ctx.importing_file));
            full.extend_from_slice(segs);
            full
        });
        Self {
            alias_map: ctx.alias_map,
            local_symbols: ctx.local_symbols,
            importing_file: ctx.importing_file,
            self_type_canonical,
            signature_params: ctx.signature_params.clone(),
            bindings: vec![HashMap::new()],
            calls: HashSet::new(),
        }
    }

    fn seed_signature_bindings(&mut self) {
        let params = self.signature_params.clone();
        for (name, ty) in &params {
            if let Some(canonical) =
                canonical_from_type(ty, self.alias_map, self.local_symbols, self.importing_file)
            {
                self.bindings[0].insert(name.clone(), canonical);
            }
        }
    }

    fn enter_scope(&mut self) {
        self.bindings.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        self.bindings.pop();
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

    fn resolve_binding(&self, ident: &str) -> Option<&Vec<String>> {
        for scope in self.bindings.iter().rev() {
            if let Some(v) = scope.get(ident) {
                return Some(v);
            }
        }
        None
    }

    /// Turn a path-segment list into the canonical String used for all
    /// call-target comparisons in the call-parity check.
    fn canonicalise_path(&self, segments: &[String]) -> String {
        if segments.is_empty() {
            return String::new();
        }
        // Self::method
        if segments[0] == "Self" {
            if let Some(self_canonical) = &self.self_type_canonical {
                let mut full = self_canonical.clone();
                full.extend_from_slice(&segments[1..]);
                return full.join("::");
            }
            return bare(&segments.join("::"));
        }
        // crate / self / super — resolve to crate-absolute
        if matches!(segments[0].as_str(), "crate" | "self" | "super") {
            if let Some(resolved) = resolve_to_crate_absolute(self.importing_file, segments) {
                let mut full = vec!["crate".to_string()];
                full.extend(resolved);
                return full.join("::");
            }
            return bare(&segments.join("::"));
        }
        // Alias-map hit on first segment → replace prefix, then
        // re-normalise in case the alias itself resolves through
        // `self::` / `super::` (e.g. `use super::foo::Bar;`).
        if let Some(alias) = self.alias_map.get(&segments[0]) {
            let mut full = alias.clone();
            full.extend_from_slice(&segments[1..]);
            if let Some(normalized) = normalize_alias_expansion(full, self.importing_file) {
                return normalized.join("::");
            }
        }
        // Same-module fallback: unqualified call whose first segment is
        // a top-level item in the same file resolves to
        // `crate::<file_module>::<segments>`. Without this, idiomatic
        // Rust like `fn helper() {}` + `pub fn cmd() { helper(); }`
        // leaves `cmd → <bare>:helper` as a dead-end edge, and Check A
        // can falsely report "no delegation" when the actual delegation
        // flows through the local helper.
        if self.local_symbols.contains(&segments[0]) {
            let mut full = vec!["crate".to_string()];
            full.extend(file_to_module_segments(self.importing_file));
            full.extend_from_slice(segments);
            return full.join("::");
        }
        // Unknown path (external crate, stdlib, or not imported) → bare.
        bare(&segments.join("::"))
    }

    fn record_call(&mut self, target: String) {
        self.calls.insert(target);
    }

    fn collect_macro_body(&mut self, mac: &syn::Macro) {
        use syn::parse::Parser;
        use syn::punctuated::Punctuated;
        use syn::Token;
        let tokens = mac.tokens.clone();
        let parser = Punctuated::<syn::Expr, Token![,]>::parse_terminated;
        if let Ok(exprs) = parser.parse2(tokens) {
            for e in exprs.into_iter() {
                self.visit_expr(&e);
            }
        }
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
        if let Some((name, ty_canonical)) = extract_let_binding(
            local,
            self.alias_map,
            self.local_symbols,
            self.importing_file,
        ) {
            self.current_scope_mut().insert(name, ty_canonical);
        }
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
        let canonical = match call.receiver.as_ref() {
            syn::Expr::Path(p) if p.path.segments.len() == 1 => {
                let ident = p.path.segments[0].ident.to_string();
                match self.resolve_binding(&ident) {
                    Some(binding) => {
                        let mut full = binding.clone();
                        full.push(method_name.clone());
                        full.join("::")
                    }
                    None => method_unknown(&method_name),
                }
            }
            _ => method_unknown(&method_name),
        };
        self.record_call(canonical);
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
