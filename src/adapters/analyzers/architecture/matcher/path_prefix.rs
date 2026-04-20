//! `forbid_path_prefix` matcher — detects paths beginning with banned prefixes.
//!
//! Walks the syn AST and reports every path reference whose rendered form
//! starts with one of the configured prefixes. Covers eight AST positions:
//!
//! 1. `use foo::Bar`
//! 2. `foo::bar(…)` (call / free path expression)
//! 3. `#[foo::attribute]`
//! 4. `impl foo::Trait for X`
//! 5. `fn baz() -> foo::Result<…>`
//! 6. `let x: foo::Type<…>` (and struct field types)
//! 7. `T: foo::Bound` (and where-clauses)
//! 8. `extern crate foo;`

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

/// Find all path-prefix matches in the given parsed file.
///
/// `prefixes` are the banned prefixes (e.g. `"tokio::"`, `"anyhow::"`).
/// Each path reference in the AST whose rendered form starts with one of
/// the prefixes yields a `MatchLocation`.
pub fn find_path_prefix_matches(
    file: &str,
    ast: &syn::File,
    prefixes: &[String],
) -> Vec<MatchLocation> {
    let mut visitor = PathPrefixVisitor {
        file,
        prefixes,
        hits: Vec::new(),
    };
    visitor.visit_file(ast);
    visitor.hits
}

struct PathPrefixVisitor<'a> {
    file: &'a str,
    prefixes: &'a [String],
    hits: Vec<MatchLocation>,
}

impl PathPrefixVisitor<'_> {
    fn check_rendered(&mut self, rendered: &str, span: proc_macro2::Span) {
        for prefix in self.prefixes {
            if matches_prefix(rendered, prefix) {
                let start = span.start();
                self.hits.push(MatchLocation {
                    file: self.file.to_string(),
                    line: start.line,
                    column: start.column,
                    kind: ViolationKind::PathPrefix {
                        prefix: prefix.clone(),
                        rendered_path: rendered.to_string(),
                    },
                });
                return;
            }
        }
    }

    fn walk_use_tree(&mut self, tree: &syn::UseTree, segments: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(p) => {
                segments.push(p.ident.to_string());
                self.walk_use_tree(&p.tree, segments);
                segments.pop();
            }
            syn::UseTree::Name(n) => {
                let rendered = join_with_leaf(segments, &n.ident.to_string());
                self.check_rendered(&rendered, n.ident.span());
            }
            syn::UseTree::Rename(r) => {
                let rendered = join_with_leaf(segments, &r.ident.to_string());
                self.check_rendered(&rendered, r.ident.span());
            }
            syn::UseTree::Glob(g) => {
                let rendered = join_with_leaf(segments, "*");
                self.check_rendered(&rendered, g.star_token.span());
            }
            syn::UseTree::Group(g) => {
                g.items.iter().for_each(|item| {
                    self.walk_use_tree(item, segments);
                });
            }
        }
    }
}

use super::render_path;

fn join_with_leaf(segments: &[String], leaf: &str) -> String {
    if segments.is_empty() {
        leaf.to_string()
    } else {
        format!("{}::{}", segments.join("::"), leaf)
    }
}

/// Match `rendered` against a configured prefix with path-segment semantics.
/// For a prefix with a trailing `::` (e.g. `"tokio::"`), both the bare
/// crate (`use tokio;` → rendered `"tokio"`) and deeper paths
/// (`use tokio::spawn;` → rendered `"tokio::spawn"`) hit. For a bare
/// prefix (`"tokio"`), the match requires an exact equality or a
/// segment boundary so `"tokios"` does not false-positive on
/// `"tokio"`.
/// Operation: allocation-free string comparison, no own calls.
fn matches_prefix(rendered: &str, prefix: &str) -> bool {
    if let Some(stripped) = prefix.strip_suffix("::") {
        rendered == stripped || rendered.starts_with(prefix)
    } else {
        // `strip_prefix` is a single pass and allocates nothing; the
        // previous `starts_with(&format!("{prefix}::"))` formed a new
        // String for every (rendered, prefix) pair — noticeable on
        // large workspaces with many configured prefixes.
        match rendered.strip_prefix(prefix) {
            Some("") => true,
            Some(rest) => rest.starts_with("::"),
            None => false,
        }
    }
}

impl<'ast> Visit<'ast> for PathPrefixVisitor<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let rendered = render_path(path);
        self.check_rendered(&rendered, path.span());
        // Recurse so generic arguments inside the path (e.g. Vec<tokio::X>) are
        // visited as independent paths.
        visit::visit_path(self, path);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let mut segments: Vec<String> = Vec::new();
        self.walk_use_tree(&node.tree, &mut segments);
        // `use` trees contain no Path nodes to descend into; skip default walk.
    }

    fn visit_item_extern_crate(&mut self, node: &'ast syn::ItemExternCrate) {
        let name = node.ident.to_string();
        for prefix in self.prefixes {
            // `extern crate foo` only exposes a single crate identifier.
            // Reuse `matches_prefix` so the trimmed-`::` form behaves the
            // same as the other matchers in this file.
            if matches_prefix(&name, prefix) {
                let start = node.ident.span().start();
                self.hits.push(MatchLocation {
                    file: self.file.to_string(),
                    line: start.line,
                    column: start.column,
                    kind: ViolationKind::PathPrefix {
                        prefix: prefix.clone(),
                        rendered_path: name.clone(),
                    },
                });
                break;
            }
        }
    }
}
