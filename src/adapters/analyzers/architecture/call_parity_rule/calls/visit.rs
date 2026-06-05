//! The `syn::Visit` walk driving call collection.

use super::super::bindings::extract_let_binding;
use super::resolve::{canonical_edges_for_method, trait_dispatch_edges};
use super::{
    bare, extract_pat_ident_name, method_unknown, parse_macro_tokens, CanonicalCallCollector,
};
use syn::spanned::Spanned;
use syn::visit::Visit;

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
        if self.try_install_annotated_binding(local) {
            return;
        }
        // The legacy `extract_let_binding` shortcut isn't mod-scope aware
        // — it would install `let s = inner::Session::new()` as
        // `crate::file::Session` while the index keys it under
        // `crate::file::inner::Session`. Skip it when a workspace index
        // is available so inference (which is scope-aware) takes over.
        if self.workspace_index.is_none() {
            if let Some((name, ty_canonical)) = extract_let_binding(
                local,
                self.file.alias_map,
                self.file.local_symbols,
                self.file.crate_root_modules,
                self.file.path,
            ) {
                self.install_path_binding(name, ty_canonical);
                return;
            }
        }
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
            let leading_colon = p.path.leading_colon.is_some();
            if let Some(targets) = self.canonicalise_generic_param_path(&segments, leading_colon) {
                for t in targets {
                    self.record_call(t);
                }
            } else {
                let canonical = self.canonicalise_path(&segments, leading_colon);
                self.record_call(canonical);
            }
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
        // Walk the macro's token stream as expressions so calls inside
        // `vec![]`, `assert!()`, etc. are still collected.
        for expr in parse_macro_tokens(mac.tokens.clone()) {
            self.visit_expr(&expr);
        }
    }

    fn visit_expr_closure(&mut self, c: &'ast syn::ExprClosure) {
        self.enter_scope();
        for input in &c.inputs {
            self.install_closure_param(input);
        }
        self.visit_expr(&c.body);
        self.exit_scope();
    }
}
