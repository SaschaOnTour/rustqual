//! `forbid_method_call` matcher — detects calls to banned method names
//! in both direct dot-notation and UFCS form.
//!
//! Coverage:
//! - `x.unwrap()` → direct method call
//! - `Option::unwrap(x)` → UFCS form, matched by final path segment
//!
//! The UFCS match is conservative: any two-segment `Path::name(...)` call
//! whose tail segment matches a banned name triggers. False positives on
//! free functions with matching names (e.g. `my_utils::unwrap(x)`) are
//! rare and addressable by `qual:allow`.

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

/// Find all method-call matches in the given parsed file.
pub fn find_method_call_matches(
    file: &str,
    ast: &syn::File,
    names: &[String],
) -> Vec<MatchLocation> {
    let mut visitor = MethodCallVisitor {
        file,
        names,
        hits: Vec::new(),
    };
    visitor.visit_file(ast);
    visitor.hits
}

struct MethodCallVisitor<'a> {
    file: &'a str,
    names: &'a [String],
    hits: Vec<MatchLocation>,
}

impl MethodCallVisitor<'_> {
    fn record(&mut self, name: &str, syntax: &'static str, span: proc_macro2::Span) {
        let start = span.start();
        self.hits.push(MatchLocation {
            file: self.file.to_string(),
            line: start.line,
            column: start.column,
            kind: ViolationKind::MethodCall {
                name: name.to_string(),
                syntax,
            },
        });
    }
}

impl<'ast> Visit<'ast> for MethodCallVisitor<'_> {
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let called = node.method.to_string();
        if self.names.iter().any(|n| n == &called) {
            self.record(&called, "direct", node.method.span());
        }
        // Descend so receiver expressions (e.g. `a.b().c()`) are still visited.
        visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        // UFCS: `Type::method(args)` → func is ExprPath with ≥2 segments and
        // the final segment matches a banned name.
        if let syn::Expr::Path(ep) = &*node.func {
            let segs = &ep.path.segments;
            if segs.len() >= 2 {
                if let Some(last) = segs.last() {
                    let name = last.ident.to_string();
                    if self.names.iter().any(|n| n == &name) {
                        self.record(&name, "ufcs", last.ident.span());
                    }
                }
            }
        }
        // Descend so inner calls in args are visited.
        visit::visit_expr_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        // Macro token streams are not parsed by syn's default visitor, but
        // we want to catch method calls inside `format!("{}", x.unwrap())`
        // and similar. Parse the token stream as a comma-separated list of
        // expressions (works for most function-like macros).
        use syn::punctuated::Punctuated;
        if let Ok(args) = syn::parse::Parser::parse2(
            Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated,
            node.tokens.clone(),
        ) {
            args.iter().for_each(|expr| visit::visit_expr(self, expr));
        }
        visit::visit_macro(self, node);
    }
}
