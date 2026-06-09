//! Per-test reference collection, shared across analyzers.
//!
//! Walks `#[test]` functions and records what each one *references* — the
//! functions/methods it calls and the type names it mentions (via
//! `Type::method()`, struct construction, or enum-variant paths). Both TQ
//! (no-SUT detection, TQ-002) and SRP (test-SUT-cohesion) need exactly this, so
//! it lives in `shared/` rather than inside one analyzer that the other reaches
//! into.

use syn::visit::Visit;

use crate::adapters::shared::cfg_test::has_test_attr;

/// What a single `#[test]` function references.
pub(crate) struct TestFnReferences {
    pub name: String,
    pub line: usize,
    /// Names called: free functions, methods (`.foo()`), `Type::assoc()`. Note
    /// method-call names are receiver-type-agnostic (ambiguous).
    pub call_targets: Vec<String>,
    /// Type names mentioned explicitly: `Type::method()`, `Type { .. }`,
    /// `Enum::Variant`. These are reliable references to a named type.
    pub type_qualified_calls: Vec<String>,
}

/// Collect the references of every `#[test]` function in one parsed file.
/// Operation: runs the collector over the file.
pub(crate) fn collect_test_references(file: &syn::File) -> Vec<TestFnReferences> {
    let mut collector = TestReferenceCollector::default();
    collector.visit_file(file);
    collector.test_fns
}

/// Collects `#[test]` functions and what each references.
#[derive(Default)]
struct TestReferenceCollector {
    test_fns: Vec<TestFnReferences>,
    in_test_fn: bool,
    current_calls: Vec<String>,
    current_type_calls: Vec<String>,
}

impl<'ast> Visit<'ast> for TestReferenceCollector {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if self.in_test_fn {
            // Macro bodies are opaque to syn's visitor; recover embedded exprs
            // so SUT references inside assert!(), vec!(), `;`-repeat / block
            // forms are seen as the test exercising production code.
            crate::adapters::shared::macro_tokens::recover_exprs(&node.tokens)
                .iter()
                .for_each(|expr| syn::visit::visit_expr(self, expr));
            // Always also harvest call/construction-position idents. A test that
            // exercises the SUT by rendering a component (`rsx!{ Component { .. }
            // }`) uses struct syntax the structured visit parses but does not
            // record as a call. Harvesting only call/construction position (not
            // prop keys) keeps it tight; for SUT detection it can only suppress a
            // false NO_SUT, never raise one.
            self.current_calls.extend(
                crate::adapters::shared::macro_tokens::idents_in_call_position(&node.tokens),
            );
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
            self.test_fns.push(TestFnReferences {
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
        // Recognize struct construction as referencing the type.
        if self.in_test_fn {
            if let Some(last) = node.path.segments.last() {
                self.current_type_calls.push(last.ident.to_string());
            }
        }
        syn::visit::visit_expr_struct(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        // Recognize enum variant paths (e.g. Dimension::Iosp → type "Dimension").
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
