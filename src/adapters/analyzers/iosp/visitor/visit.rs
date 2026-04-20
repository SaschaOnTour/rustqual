use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMethodCall};

use super::{is_delegation_only_body, is_match_dispatch, is_trivial_match_arm, BodyVisitor};
use crate::adapters::analyzers::iosp::types::CallOccurrence;

impl<'a, 'ast> Visit<'ast> for BodyVisitor<'a> {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        // Reset boolean alternation tracking for non-boolean expressions
        if !matches!(expr, Expr::Binary(b) if matches!(b.op, syn::BinOp::And(_) | syn::BinOp::Or(_)))
        {
            self.last_boolean_op = None;
        }

        match expr {
            // --- Logic detection ---
            Expr::If(expr_if) => {
                // Complexity: always tracked (regardless of closure leniency)
                self.cognitive_complexity += 1 + self.nesting_depth;
                self.cyclomatic_complexity += 1;
                self.record_hotspot("if", expr_if.if_token.span);
                // IOSP: respects closure leniency
                self.record_logic("if", expr_if.if_token.span);
                self.enter_nesting();
                syn::visit::visit_expr(self, expr);
                self.exit_nesting();
                return;
            }
            Expr::Match(expr_match) => {
                self.cognitive_complexity += 1 + self.nesting_depth;
                // Cyclomatic: only non-trivial arms count (trivial lookup arms excluded)
                let non_trivial = expr_match
                    .arms
                    .iter()
                    .filter(|arm| !is_trivial_match_arm(arm))
                    .count();
                self.cyclomatic_complexity += non_trivial.saturating_sub(1);
                self.record_hotspot("match", expr_match.match_token.span);
                if !is_match_dispatch(&expr_match.arms) {
                    self.record_logic("match", expr_match.match_token.span);
                }
                self.enter_nesting();
                syn::visit::visit_expr(self, expr);
                self.exit_nesting();
                return;
            }
            Expr::ForLoop(expr_for) => {
                self.cognitive_complexity += 1 + self.nesting_depth;
                self.cyclomatic_complexity += 1;
                self.record_hotspot("for", expr_for.for_token.span);
                if !is_delegation_only_body(&expr_for.body.stmts) {
                    self.record_logic("for", expr_for.for_token.span);
                }
                // Skip logic detection in the iterator expression (range, .len(), etc.)
                self.in_for_iter = true;
                self.visit_expr(&expr_for.expr);
                self.in_for_iter = false;
                self.visit_pat(&expr_for.pat);
                self.enter_nesting();
                self.visit_block(&expr_for.body);
                self.exit_nesting();
                return;
            }
            Expr::While(expr_while) => {
                self.cognitive_complexity += 1 + self.nesting_depth;
                self.cyclomatic_complexity += 1;
                self.record_hotspot("while", expr_while.while_token.span);
                self.record_logic("while", expr_while.while_token.span);
                self.enter_nesting();
                syn::visit::visit_expr(self, expr);
                self.exit_nesting();
                return;
            }
            Expr::Loop(expr_loop) => {
                self.cognitive_complexity += 1 + self.nesting_depth;
                self.cyclomatic_complexity += 1;
                self.record_hotspot("loop", expr_loop.loop_token.span);
                self.record_logic("loop", expr_loop.loop_token.span);
                self.enter_nesting();
                syn::visit::visit_expr(self, expr);
                self.exit_nesting();
                return;
            }

            // --- ? operator as logic (when strict_error_propagation enabled) ---
            Expr::Try(expr_try) => {
                if self.config.strict_error_propagation {
                    self.record_logic("?", expr_try.question_token.span());
                }
                syn::visit::visit_expr(self, expr);
                return;
            }

            // --- Arithmetic / boolean operators as logic ---
            Expr::Binary(expr_bin) => {
                use syn::BinOp;
                let kind = match &expr_bin.op {
                    BinOp::Add(_)
                    | BinOp::Sub(_)
                    | BinOp::Mul(_)
                    | BinOp::Div(_)
                    | BinOp::Rem(_) => Some("arithmetic"),
                    BinOp::And(_) | BinOp::Or(_) => Some("boolean_op"),
                    BinOp::Eq(_)
                    | BinOp::Ne(_)
                    | BinOp::Lt(_)
                    | BinOp::Le(_)
                    | BinOp::Gt(_)
                    | BinOp::Ge(_) => Some("comparison"),
                    BinOp::BitAnd(_)
                    | BinOp::BitOr(_)
                    | BinOp::BitXor(_)
                    | BinOp::Shl(_)
                    | BinOp::Shr(_) => Some("bitwise"),
                    _ => None,
                };
                if let Some(kind) = kind {
                    self.record_logic(kind, expr_bin.op.span());
                }
                // Complexity: boolean operators add to cyclomatic and track alternation
                match &expr_bin.op {
                    BinOp::And(_) => {
                        self.cyclomatic_complexity += 1;
                        let is_and = true;
                        if let Some(last) = self.last_boolean_op {
                            if last != is_and {
                                self.cognitive_complexity += 1;
                            }
                        }
                        self.last_boolean_op = Some(is_and);
                    }
                    BinOp::Or(_) => {
                        self.cyclomatic_complexity += 1;
                        let is_and = false;
                        if let Some(last) = self.last_boolean_op {
                            if last != is_and {
                                self.cognitive_complexity += 1;
                            }
                        }
                        self.last_boolean_op = Some(is_and);
                    }
                    _ => {}
                }
                syn::visit::visit_expr(self, expr);
                return;
            }

            // --- Unsafe blocks: count occurrences ---
            Expr::Unsafe(_) => {
                self.unsafe_block_count += 1;
                syn::visit::visit_expr(self, expr);
                return;
            }

            // --- Closures: track depth ---
            Expr::Closure(_) => {
                self.closure_depth += 1;
                syn::visit::visit_expr(self, expr);
                self.closure_depth -= 1;
                return;
            }

            // --- Async blocks: track depth (treated like closures) ---
            Expr::Async(_) => {
                self.async_block_depth += 1;
                syn::visit::visit_expr(self, expr);
                self.async_block_depth -= 1;
                return;
            }

            // --- .await is not logic ---
            Expr::Await(_) => {
                syn::visit::visit_expr(self, expr);
                return;
            }

            // --- Array index: skip magic number detection for index expression ---
            Expr::Index(expr_index) => {
                self.visit_expr(&expr_index.expr);
                self.in_index_context += 1;
                self.visit_expr(&expr_index.index);
                self.in_index_context -= 1;
                return;
            }

            // --- Numeric literals: magic number detection ---
            Expr::Lit(expr_lit) => {
                match &expr_lit.lit {
                    syn::Lit::Int(lit_int) => {
                        self.record_magic_number(
                            lit_int.base10_digits().to_string(),
                            lit_int.span(),
                        );
                    }
                    syn::Lit::Float(lit_float) => {
                        self.record_magic_number(
                            lit_float.base10_digits().to_string(),
                            lit_float.span(),
                        );
                    }
                    _ => {}
                }
                syn::visit::visit_expr(self, expr);
                return;
            }

            // --- Unary negation: detect negative magic numbers like -1 ---
            // Handle as a unit to avoid double-counting the inner positive literal.
            Expr::Unary(expr_unary) if matches!(expr_unary.op, syn::UnOp::Neg(_)) => {
                if let Expr::Lit(expr_lit) = &*expr_unary.expr {
                    match &expr_lit.lit {
                        syn::Lit::Int(lit_int) => {
                            self.record_magic_number(
                                format!("-{}", lit_int.base10_digits()),
                                lit_int.span(),
                            );
                            // Skip default recursion — we already handled the inner literal
                            return;
                        }
                        syn::Lit::Float(lit_float) => {
                            self.record_magic_number(
                                format!("-{}", lit_float.base10_digits()),
                                lit_float.span(),
                            );
                            return;
                        }
                        _ => {}
                    }
                }
                syn::visit::visit_expr(self, expr);
                return;
            }

            // --- Macro detection for error handling: panic!/todo!/unreachable! ---
            Expr::Macro(expr_macro) => {
                let macro_name = expr_macro
                    .mac
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string());
                match macro_name.as_deref() {
                    Some("panic") => self.panic_count += 1,
                    Some("unreachable") => self.panic_count += 1,
                    Some("todo") => self.todo_count += 1,
                    _ => {}
                }
                syn::visit::visit_expr(self, expr);
                return;
            }

            // --- Function call detection ---
            Expr::Call(ExprCall { func, .. }) => {
                if self.in_lenient_nested_context() {
                    // skip in lenient closure/async mode
                } else if let Some(name) = Self::extract_call_name(func) {
                    if self.config.allow_recursion && self.is_recursive_call(&name) {
                        // Recursive call — skip when allow_recursion is enabled
                    } else if self.scope.is_own_function(&name) {
                        self.own_calls.push(CallOccurrence {
                            name,
                            line: func.span().start().line,
                        });
                    }
                }
                syn::visit::visit_expr(self, expr);
                return;
            }

            Expr::MethodCall(ExprMethodCall {
                method, receiver, ..
            }) => {
                let method_name = method.to_string();
                // Error handling: count in ALL contexts (including closures)
                match method_name.as_str() {
                    "unwrap" => self.unwrap_count += 1,
                    "expect" => self.expect_count += 1,
                    _ => {}
                }
                if self.in_lenient_nested_context() {
                    // skip in lenient closure/async mode
                } else {
                    let is_iterator = Self::is_iterator_method(&method_name);
                    if is_iterator && !self.config.strict_iterator_chains {
                        // skip — iterator adaptor, not an "own call"
                    } else if self.config.allow_recursion && self.is_recursive_call(&method_name) {
                        // Recursive call — skip
                    } else if self.is_type_resolved_own_method(&method_name, receiver) {
                        self.own_calls.push(CallOccurrence {
                            name: format!(".{method_name}()"),
                            line: method.span().start().line,
                        });
                    }
                }
                syn::visit::visit_expr(self, expr);
                return;
            }

            _ => {}
        }

        // Default: recurse into children
        syn::visit::visit_expr(self, expr);
    }

    /// Track const item context to suppress magic number detection.
    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        self.in_const_context += 1;
        syn::visit::visit_item_const(self, i);
        self.in_const_context -= 1;
    }

    /// Track static item context to suppress magic number detection.
    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        self.in_const_context += 1;
        syn::visit::visit_item_static(self, i);
        self.in_const_context -= 1;
    }
}
