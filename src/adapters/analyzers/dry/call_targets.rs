use std::collections::HashSet;

use syn::visit::Visit;

// ── Call target collection ──────────────────────────────────────

/// Collect all function/method call targets from all parsed files,
/// separated into production and test contexts.
/// Trivial: creates visitor and delegates via for_each closure.
pub(crate) fn collect_all_calls(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    let mut collector = CallTargetCollector {
        production_calls: HashSet::new(),
        test_calls: HashSet::new(),
        in_test: false,
    };
    parsed.iter().for_each(|(path, _, file)| {
        collector.in_test = cfg_test_files.contains(path);
        syn::visit::visit_file(&mut collector, file);
    });
    (collector.production_calls, collector.test_calls)
}

/// AST visitor that collects all function/method call targets.
struct CallTargetCollector {
    production_calls: HashSet<String>,
    test_calls: HashSet<String>,
    in_test: bool,
}

/// Insert the last path segment and qualified `Type::method` form into the target set.
fn insert_path_segments(target: &mut HashSet<String>, path: &syn::Path) {
    let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    if let Some(last) = segments.last() {
        target.insert(last.clone());
    }
    if segments.len() >= 2 {
        target.insert(format!(
            "{}::{}",
            segments[segments.len() - 2],
            segments.last().unwrap()
        ));
    }
}

impl CallTargetCollector {
    /// Extract function names referenced by serde field attributes.
    /// Operation: attribute parsing logic, no own calls.
    fn extract_serde_fn_refs(attrs: &[syn::Attribute]) -> Vec<String> {
        let mut refs = Vec::new();
        let push_fn_ref = |refs: &mut Vec<String>, s: String| {
            if let Some(name) = s.rsplit("::").next() {
                refs.push(name.to_string());
            }
            if s.contains("::") {
                refs.push(s);
            }
        };
        attrs
            .iter()
            .filter(|a| a.path().is_ident("serde"))
            .for_each(|attr| {
                let _ = attr.parse_nested_meta(|meta| {
                    let is_fn_key = meta.path.is_ident("deserialize_with")
                        || meta.path.is_ident("serialize_with")
                        || meta.path.is_ident("default");
                    if is_fn_key || meta.path.is_ident("with") {
                        if let Ok(value) = meta.value() {
                            if let Ok(lit) = value.parse::<syn::LitStr>() {
                                let s = lit.value();
                                if is_fn_key {
                                    push_fn_ref(&mut refs, s);
                                } else {
                                    refs.push(format!("{s}::serialize"));
                                    refs.push(format!("{s}::deserialize"));
                                    refs.extend(["serialize".into(), "deserialize".into()]);
                                }
                            }
                        }
                    }
                    Ok(())
                });
            });
        refs
    }

    /// Extract function references from call arguments (e.g., `.for_each(some_fn)`).
    fn record_path_args(
        &mut self,
        args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    ) {
        let target = if self.in_test {
            &mut self.test_calls
        } else {
            &mut self.production_calls
        };
        args.iter().for_each(|arg| {
            let expr = match arg {
                syn::Expr::Reference(r) => &*r.expr,
                other => other,
            };
            if let syn::Expr::Path(p) = expr {
                insert_path_segments(target, &p.path);
            }
        });
    }
}

impl<'ast> Visit<'ast> for CallTargetCollector {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            let target = if self.in_test {
                &mut self.test_calls
            } else {
                &mut self.production_calls
            };
            insert_path_segments(target, &p.path);
        }
        self.record_path_args(&node.args);
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let name = node.method.to_string();
        if self.in_test {
            self.test_calls.insert(name);
        } else {
            self.production_calls.insert(name);
        }
        self.record_path_args(&node.args);
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        let target = if self.in_test {
            &mut self.test_calls
        } else {
            &mut self.production_calls
        };
        node.fields.iter().for_each(|field| {
            if let syn::Expr::Path(p) = &field.expr {
                insert_path_segments(target, &p.path);
            }
        });
        syn::visit::visit_expr_struct(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let prev = self.in_test;
        if super::has_cfg_test(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_test = prev;
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let prev = self.in_test;
        if super::has_test_attr(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_fn(self, node);
        self.in_test = prev;
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        // Parse macro arguments as expressions to find embedded function calls.
        // Works for assert!(), assert_eq!(), format!(), vec![], etc.
        use syn::punctuated::Punctuated;
        if let Ok(args) = syn::parse::Parser::parse2(
            Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
            node.tokens.clone(),
        ) {
            args.iter()
                .for_each(|expr| syn::visit::visit_expr(self, expr));
        }
        syn::visit::visit_macro(self, node);
    }

    fn visit_field(&mut self, node: &'ast syn::Field) {
        let refs = Self::extract_serde_fn_refs(&node.attrs);
        if self.in_test {
            self.test_calls.extend(refs);
        } else {
            self.production_calls.extend(refs);
        }
        syn::visit::visit_field(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        // Only pub/pub(crate) re-exports count as usage of the original function.
        // Private `use` imports are not re-exports; their call targets are already
        // captured via visit_expr_call when the imported name is actually called.
        if matches!(node.vis, syn::Visibility::Inherited) {
            return;
        }
        let target = if self.in_test {
            &mut self.test_calls
        } else {
            &mut self.production_calls
        };
        // Iterative UseTree walk
        let mut stack: Vec<&syn::UseTree> = vec![&node.tree];
        while let Some(tree) = stack.pop() {
            match tree {
                syn::UseTree::Name(n) => {
                    target.insert(n.ident.to_string());
                }
                syn::UseTree::Rename(r) => {
                    // Record the ORIGINAL name (r.ident), not the alias (r.rename).
                    target.insert(r.ident.to_string());
                }
                syn::UseTree::Path(p) => stack.push(&p.tree),
                syn::UseTree::Group(g) => stack.extend(&g.items),
                syn::UseTree::Glob(_) => {} // Can't enumerate; skip
            }
        }
        // No need to recurse — ItemUse has no child expressions to visit.
    }
}
