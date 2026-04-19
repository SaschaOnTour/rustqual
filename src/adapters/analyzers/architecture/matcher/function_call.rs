//! `forbid_function_call` matcher — detects calls to banned function paths.
//!
//! Where `forbid_method_call` matches by the **final** segment of a call
//! (so `unwrap` fires for both `x.unwrap()` and `Option::unwrap(x)`), this
//! matcher compares the **full rendered path** of a call-expression's
//! function target. It is the natural shape for rules like
//! `forbid_function_call = ["Box::new", "std::process::exit"]`.
//!
//! What it looks at:
//!   - `syn::Expr::Call` where `func` is `syn::Expr::Path`.
//!   - Rendered form of the path (`a::b::c`) is compared to each
//!     configured string for equality.
//!
//! What it ignores:
//!   - Method calls (`x.name(...)`) — those are `forbid_method_call`
//!     territory and would otherwise double-report.
//!   - Macro invocations (`name!(...)`) — handled by `forbid_macro_call`.
//!   - Closure invocations and turbofish-wrapped paths are still walked
//!     but compared literally (turbofish angles are stripped from the
//!     rendered form so `foo::<T>` compares as `foo`).

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

/// Find all function-call matches in the given parsed file.
///
/// `paths` are the banned fully-qualified path strings (e.g. `"Box::new"`).
/// Each free-function or static-method call whose rendered path equals one
/// of the configured strings yields a `MatchLocation`.
pub fn find_function_call_matches(
    file: &str,
    ast: &syn::File,
    paths: &[String],
) -> Vec<MatchLocation> {
    let mut visitor = FunctionCallVisitor {
        file,
        paths,
        hits: Vec::new(),
    };
    visitor.visit_file(ast);
    visitor.hits
}

struct FunctionCallVisitor<'a> {
    file: &'a str,
    paths: &'a [String],
    hits: Vec<MatchLocation>,
}

impl FunctionCallVisitor<'_> {
    fn record(&mut self, rendered: &str, span: proc_macro2::Span) {
        let start = span.start();
        self.hits.push(MatchLocation {
            file: self.file.to_string(),
            line: start.line,
            column: start.column,
            kind: ViolationKind::FunctionCall {
                rendered_path: rendered.to_string(),
            },
        });
    }
}

impl<'ast> Visit<'ast> for FunctionCallVisitor<'_> {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(ep) = &*node.func {
            let rendered = render_path(&ep.path);
            if self.paths.iter().any(|p| p == &rendered) {
                self.record(&rendered, ep.path.span());
            }
        }
        visit::visit_expr_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        // Macro token streams are invisible to the default visitor. Parse
        // the token stream as comma-separated expressions so calls inside
        // `format!("{}", Box::new(x))` are caught.
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

/// Render a `syn::Path` as `a::b::c`, ignoring turbofish/generic arguments.
/// Operation: iterator-chain join.
fn render_path(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}
