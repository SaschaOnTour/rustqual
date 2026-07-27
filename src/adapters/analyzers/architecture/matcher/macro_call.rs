//! `forbid_macro_call` matcher — detects macro invocations by name.
//!
//! Matches by the **final path segment** of the macro's invocation path,
//! so both `println!` and `std::println!` count as "println" for the
//! purpose of the rule. Plain function calls with the same name are not
//! matched (macros are a distinct AST node).

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
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
        // `vec![format!(...)]`) are caught — recover the embedded exprs
        // (comma-list, `;`-repeat, and block-bodied forms) via the shared
        // helper the rustqual call-target collector also uses. Structured-only
        // by design: a forbid-matcher must NOT use the raw positional fallback
        // (it would manufacture false violations), so a nested macro inside an
        // unparseable extern-DSL body is best-effort-missed.
        for expr in crate::adapters::shared::macro_tokens::recover_exprs(&node.tokens) {
            visit::visit_expr(self, &expr);
        }
        visit::visit_macro(self, node);
    }
}
