//! `forbid_glob_import` matcher — detects `use foo::*` glob imports.
//!
//! Walks every `use`-tree and reports each `UseTree::Glob` as a
//! `MatchLocation` carrying the base path that precedes the `*`.

use crate::architecture::{MatchLocation, ViolationKind};
use syn::spanned::Spanned;
use syn::visit::Visit;

/// Find all glob imports in the given parsed file.
///
/// A glob import is any `use some::path::*` statement — including
/// `use self::*`, `use super::*`, and globs inside use-groups such as
/// `use foo::{bar::*, baz}`.
pub fn find_glob_imports(file: &str, ast: &syn::File) -> Vec<MatchLocation> {
    let mut visitor = GlobImportVisitor {
        file,
        hits: Vec::new(),
    };
    visitor.visit_file(ast);
    visitor.hits
}

struct GlobImportVisitor<'a> {
    file: &'a str,
    hits: Vec<MatchLocation>,
}

impl GlobImportVisitor<'_> {
    fn walk_use_tree(&mut self, tree: &syn::UseTree, segments: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(p) => {
                segments.push(p.ident.to_string());
                self.walk_use_tree(&p.tree, segments);
                segments.pop();
            }
            syn::UseTree::Glob(g) => {
                let base_path = segments.join("::");
                let start = g.star_token.span().start();
                self.hits.push(MatchLocation {
                    file: self.file.to_string(),
                    line: start.line,
                    column: start.column,
                    kind: ViolationKind::GlobImport { base_path },
                });
            }
            syn::UseTree::Group(g) => {
                g.items.iter().for_each(|item| {
                    self.walk_use_tree(item, segments);
                });
            }
            syn::UseTree::Name(_) | syn::UseTree::Rename(_) => {
                // not a glob — ignore
            }
        }
    }
}

impl<'ast> Visit<'ast> for GlobImportVisitor<'_> {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let mut segments = Vec::new();
        self.walk_use_tree(&node.tree, &mut segments);
    }
}
