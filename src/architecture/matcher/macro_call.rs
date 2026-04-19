//! `forbid_macro_call` matcher — detects macro invocations by name.
//!
//! Matches by the **final path segment** of the macro's invocation path,
//! so both `println!` and `std::println!` count as "println" for the
//! purpose of the rule. Plain function calls with the same name are not
//! matched (macros are a distinct AST node).

use crate::architecture::{MatchLocation, ViolationKind};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

/// Find all macro invocations whose final path segment matches one of `names`.
pub fn find_macro_calls(file: &str, ast: &syn::File, names: &[String]) -> Vec<MatchLocation> {
    let mut visitor = MacroCallVisitor {
        file,
        names,
        hits: Vec::new(),
    };
    visitor.visit_file(ast);
    visitor.hits
}

struct MacroCallVisitor<'a> {
    file: &'a str,
    names: &'a [String],
    hits: Vec<MatchLocation>,
}

impl<'ast> Visit<'ast> for MacroCallVisitor<'_> {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if let Some(last) = node.path.segments.last() {
            let name = last.ident.to_string();
            if self.names.iter().any(|n| n == &name) {
                let start = last.ident.span().start();
                self.hits.push(MatchLocation {
                    file: self.file.to_string(),
                    line: start.line,
                    column: start.column,
                    kind: ViolationKind::MacroCall { name },
                });
            }
        }
        // Descend into the macro token stream so nested macros (e.g.
        // `vec![format!(...)]`) are caught. Matches the same parse trick
        // the rustqual call-target collector uses.
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
