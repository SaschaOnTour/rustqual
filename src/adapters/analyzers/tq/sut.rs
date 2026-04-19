use std::collections::HashSet;

use syn::visit::Visit;

use crate::adapters::analyzers::dry::DeclaredFunction;
use crate::adapters::analyzers::iosp::scope::ProjectScope;

use super::{TqWarning, TqWarningKind};

/// Detect test functions that do not call any production function (TQ-002).
/// Operation: iterates test functions, compares call targets against known prod functions.
pub(crate) fn detect_no_sut_tests(
    parsed: &[(String, String, syn::File)],
    scope: &ProjectScope,
    declared_fns: &[DeclaredFunction],
    reaches_prod: &HashSet<String>,
) -> Vec<TqWarning> {
    // Build set of known production function names
    let prod_fn_names: HashSet<&str> = declared_fns
        .iter()
        .filter(|f| !f.is_test)
        .map(|f| f.name.as_str())
        .collect();

    let mut warnings = Vec::new();
    for (path, _, syntax) in parsed {
        let mut collector = TestCallCollector::default();
        collector.visit_file(syntax);
        for test_fn in &collector.test_fns {
            let calls_prod = test_fn.call_targets.iter().any(|target| {
                prod_fn_names.contains(target.as_str())
                    || scope.functions.contains(target)
                    || scope.methods.contains(target)
                    || reaches_prod.contains(target)
            }) || test_fn
                .type_qualified_calls
                .iter()
                .any(|type_name| scope.types.contains(type_name));
            if !calls_prod {
                warnings.push(TqWarning {
                    file: path.clone(),
                    line: test_fn.line,
                    function_name: test_fn.name.clone(),
                    kind: TqWarningKind::NoSut,
                    suppressed: false,
                });
            }
        }
    }
    warnings
}

/// A collected test function with its call targets.
struct TestFnWithCalls {
    name: String,
    line: usize,
    call_targets: Vec<String>,
    /// Type names from `Type::method()` calls (for recognizing constructor/static method calls).
    type_qualified_calls: Vec<String>,
}

/// Collects `#[test]` functions and the functions they call.
#[derive(Default)]
struct TestCallCollector {
    test_fns: Vec<TestFnWithCalls>,
    in_test_fn: bool,
    current_calls: Vec<String>,
    current_type_calls: Vec<String>,
}

impl<'ast> Visit<'ast> for TestCallCollector {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self.in_test_fn {
            // Parse macro arguments (assert!, assert_eq!, vec!, etc.) as expressions
            // to find function calls embedded inside macros.
            use syn::punctuated::Punctuated;
            if let Ok(args) = syn::parse::Parser::parse2(
                Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
                node.tokens.clone(),
            ) {
                args.iter()
                    .for_each(|expr| syn::visit::visit_expr(self, expr));
            }
        }
        syn::visit::visit_macro(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if has_test_attr(&node.attrs) {
            self.in_test_fn = true;
            self.current_calls.clear();
            self.current_type_calls.clear();
            syn::visit::visit_item_fn(self, node);
            self.in_test_fn = false;
            let line = node.sig.ident.span().start().line;
            self.test_fns.push(TestFnWithCalls {
                name: node.sig.ident.to_string(),
                line,
                call_targets: std::mem::take(&mut self.current_calls),
                type_qualified_calls: std::mem::take(&mut self.current_type_calls),
            });
        } else {
            syn::visit::visit_item_fn(self, node);
        }
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.in_test_fn {
            if let syn::Expr::Path(ref p) = *node.func {
                let name = path_to_name(&p.path);
                self.current_calls.push(name);
                if let Some(type_name) = path_type_prefix(&p.path) {
                    self.current_type_calls.push(type_name);
                }
            }
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if self.in_test_fn {
            self.current_calls.push(node.method.to_string());
        }
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        // Recognize production struct construction as exercising SUT
        if self.in_test_fn {
            if let Some(last) = node.path.segments.last() {
                self.current_type_calls.push(last.ident.to_string());
            }
        }
        syn::visit::visit_expr_struct(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        // Recognize enum variant paths (e.g. Dimension::Iosp → type "Dimension")
        if self.in_test_fn && node.path.segments.len() >= 2 {
            let type_seg = &node.path.segments[node.path.segments.len() - 2];
            self.current_type_calls.push(type_seg.ident.to_string());
        }
        syn::visit::visit_expr_path(self, node);
    }
}

/// Extract the final segment name from a path.
/// Operation: path segment extraction.
fn path_to_name(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default()
}

/// Extract the type prefix from a 2+-segment path (e.g. "Config" from "Config::load").
/// Operation: path prefix extraction.
fn path_type_prefix(path: &syn::Path) -> Option<String> {
    let len = path.segments.len();
    if len >= 2 {
        path.segments
            .iter()
            .nth(len - 2)
            .map(|s| s.ident.to_string())
    } else {
        None
    }
}

use crate::adapters::analyzers::dry::has_test_attr;
