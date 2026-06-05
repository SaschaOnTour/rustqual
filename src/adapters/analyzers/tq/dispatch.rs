//! Visitor-dispatch call-graph edges for TQ-003 reachability.
//!
//! TQ's call graph is built from direct `self.method()` / `func()` calls, so it
//! cannot follow `syn`'s trait dispatch: a test that drives a visitor via
//! `collector.visit_block(body)` reaches the *entry* (`visit_block`), but syn's
//! internal `visit_block → visit_stmt → visit_expr` recursion into the type's
//! overrides is invisible. The overridden `visit_*` methods (and the helper
//! methods they call) therefore look unreachable from tests.
//!
//! This module restores the missing reachability with **real edges**, not a
//! blanket "visitors are tested" assumption: for each visitor type `T` it
//! records the helper methods `T`'s `syn::Visit` overrides call, and at each
//! drive-site (`x.visit_block(..)`, `syn::visit::visit_*(&mut x, ..)`,
//! `visit_all_files(.., &mut x)`) it resolves the driven type `T` locally and
//! adds `driver_fn → helpers(T)` edges. Testedness then flows only when a test
//! actually drives the visitor — a visitor no test exercises stays untested.

use std::collections::HashMap;

use syn::visit::Visit;

/// Known generic forwarder helpers whose visitor argument is the *driven* type.
/// `visit_all_files(parsed, &mut collector)` drives `collector` over every file.
const DRIVE_FORWARDERS: &[(&str, usize)] = &[("visit_all_files", 1)];

/// Compute `driver_fn → helper-method-name` edges that model syn-visitor
/// dispatch. Each returned pair adds the helper names a driver transitively
/// reaches by launching its visitor; the bare call graph propagates from there.
pub(crate) fn visitor_dispatch_edges(
    parsed: &[(String, String, syn::File)],
) -> Vec<(String, Vec<String>)> {
    let helpers = collect_visitor_helpers(parsed);
    let mut drives = DriveCollector {
        helpers: &helpers,
        edges: Vec::new(),
        current_fn: None,
        bindings: HashMap::new(),
    };
    for (_, _, file) in parsed {
        drives.visit_file(file);
    }
    drives.edges
}

/// The last path segment's identifier (the unqualified type / fn name).
fn last_segment(path: &syn::Path) -> Option<String> {
    path.segments.last().map(|s| s.ident.to_string())
}

/// The unqualified name of a `Type::Path` self-type (`Foo<'a>` → `Foo`).
fn self_type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(tp) => last_segment(&tp.path),
        _ => None,
    }
}

/// If `expr` is a visitor construction (`T::new(..)`, `T::default()`,
/// `T { .. }`), return `T`. Used both for `let` bindings and inline drives.
fn constructed_type(expr: &syn::Expr) -> Option<String> {
    match expr {
        syn::Expr::Struct(s) => last_segment(&s.path),
        syn::Expr::Call(c) => match c.func.as_ref() {
            syn::Expr::Path(p) if p.path.segments.len() >= 2 => {
                let segs = &p.path.segments;
                let last = segs.last()?.ident.to_string();
                let is_ctor =
                    matches!(last.as_str(), "new" | "default" | "with_capacity" | "build");
                is_ctor.then(|| segs[segs.len() - 2].ident.to_string())
            }
            _ => None,
        },
        _ => None,
    }
}

// ── Pass 1: per-type visitor helper methods ─────────────────────

/// AST visitor collecting, per type `T`, the helper method names that `T`'s
/// `syn::Visit` override methods call (everything they invoke that isn't itself
/// a `visit_*` method).
#[derive(Default)]
struct HelperCollector {
    map: HashMap<String, Vec<String>>,
    /// Stack of enclosing-impl visitor types (`None` = not a `Visit` impl).
    type_stack: Vec<Option<String>>,
    /// `Some(T)` while inside a `visit_*` override of `Visit`-impl type `T`.
    collecting: Option<String>,
}

impl HelperCollector {
    /// Record a callee name as a helper of the type currently being collected.
    /// Operation: gate on collecting state + non-`visit_` name, then insert.
    fn record(&mut self, name: &str) {
        if let Some(t) = self.collecting.clone() {
            if !name.starts_with("visit_") {
                self.map.entry(t).or_default().push(name.to_string());
            }
        }
    }
}

impl<'ast> Visit<'ast> for HelperCollector {
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let is_visit = node
            .trait_
            .as_ref()
            .is_some_and(|(_, path, _)| last_segment(path).is_some_and(|s| s.starts_with("Visit")));
        let ty = is_visit.then(|| self_type_name(&node.self_ty)).flatten();
        self.type_stack.push(ty);
        syn::visit::visit_item_impl(self, node);
        self.type_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let enclosing = self.type_stack.last().cloned().flatten();
        let is_visit_method = node.sig.ident.to_string().starts_with("visit_");
        let prev = self.collecting.take();
        if let Some(t) = enclosing {
            if is_visit_method {
                self.collecting = Some(t);
            }
        }
        syn::visit::visit_impl_item_fn(self, node);
        self.collecting = prev;
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.record(&node.method.to_string());
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = node.func.as_ref() {
            if let Some(last) = last_segment(&p.path) {
                self.record(&last);
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}

/// Run pass 1 over all files.
/// Operation: per-file visitor drive, no own calls.
fn collect_visitor_helpers(parsed: &[(String, String, syn::File)]) -> HashMap<String, Vec<String>> {
    let mut collector = HelperCollector::default();
    for (_, _, file) in parsed {
        collector.visit_file(file);
    }
    collector.map
}

// ── Pass 2: drive-sites → helper edges ──────────────────────────

/// AST visitor that, per function, tracks local visitor constructions and emits
/// `driver_fn → helpers(T)` edges for each drive-site whose visitor resolves to
/// a known visitor type `T`.
struct DriveCollector<'h> {
    helpers: &'h HashMap<String, Vec<String>>,
    edges: Vec<(String, Vec<String>)>,
    current_fn: Option<String>,
    /// Local `let v = T::new()` bindings in the current function: ident → type.
    bindings: HashMap<String, String>,
}

impl DriveCollector<'_> {
    /// Resolve the visitor expression at a drive-site to its type name, via the
    /// local binding table or an inline construction. `&mut x` is unwrapped.
    /// Operation: reference peel + binding lookup / inline ctor.
    fn resolve_driven(&self, expr: &syn::Expr) -> Option<String> {
        let inner = match expr {
            syn::Expr::Reference(r) => r.expr.as_ref(),
            other => other,
        };
        if let Some(t) = constructed_type(inner) {
            return Some(t);
        }
        if let syn::Expr::Path(p) = inner {
            if let Some(id) = p.path.get_ident() {
                return self.bindings.get(&id.to_string()).cloned();
            }
        }
        None
    }

    /// Emit a `driver → helpers(T)` edge if `T` is a known visitor type.
    /// Operation: map lookup + push.
    fn emit_drive(&mut self, driven_type: Option<String>) {
        let (Some(t), Some(caller)) = (driven_type, self.current_fn.clone()) else {
            return;
        };
        if let Some(hs) = self.helpers.get(&t) {
            self.edges.push((caller, hs.clone()));
        }
    }

    /// Record `let v = <ctor>` so later drives on `v` resolve to its type.
    /// Operation: pattern/ident extraction + insert.
    fn record_binding(&mut self, local: &syn::Local) {
        let syn::Pat::Ident(pi) = &local.pat else {
            return;
        };
        if let Some(init) = &local.init {
            if let Some(t) = constructed_type(&init.expr) {
                self.bindings.insert(pi.ident.to_string(), t);
            }
        }
    }

    /// Handle a free-fn call that may drive a visitor: `syn::visit::visit_*(x,..)`
    /// or a known forwarder like `visit_all_files(.., &mut x)`.
    /// Operation: name dispatch + argument resolution.
    fn handle_call_drive(&mut self, node: &syn::ExprCall) {
        let syn::Expr::Path(p) = node.func.as_ref() else {
            return;
        };
        let Some(name) = last_segment(&p.path) else {
            return;
        };
        let is_syn_visit =
            name.starts_with("visit_") && p.path.segments.iter().any(|s| s.ident == "visit");
        if is_syn_visit {
            let driven = node.args.first().and_then(|a| self.resolve_driven(a));
            self.emit_drive(driven);
            return;
        }
        if let Some((_, arg_idx)) = DRIVE_FORWARDERS.iter().find(|(n, _)| *n == name) {
            let driven = node
                .args
                .iter()
                .nth(*arg_idx)
                .and_then(|a| self.resolve_driven(a));
            self.emit_drive(driven);
        }
    }

    /// Enter a function: reset the per-function binding table and walk the body.
    /// Operation: save/restore current_fn + bindings around the recursion.
    fn enter_fn(&mut self, name: String, walk: impl FnOnce(&mut Self)) {
        let prev_fn = self.current_fn.take();
        let prev_bindings = std::mem::take(&mut self.bindings);
        self.current_fn = Some(name);
        walk(self);
        self.current_fn = prev_fn;
        self.bindings = prev_bindings;
    }
}

impl<'ast> Visit<'ast> for DriveCollector<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        self.enter_fn(name, |s| syn::visit::visit_item_fn(s, node));
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        self.enter_fn(name, |s| syn::visit::visit_impl_item_fn(s, node));
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        self.record_binding(node);
        syn::visit::visit_local(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method.to_string().starts_with("visit_") {
            let driven = self.resolve_driven(&node.receiver);
            self.emit_drive(driven);
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        self.handle_call_drive(node);
        syn::visit::visit_expr_call(self, node);
    }
}
