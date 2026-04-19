//! `forbid_derive` matcher — detects `#[derive(Name)]` with banned traits.
//!
//! Walks every struct/enum/union at any nesting level, and for each
//! `#[derive(...)]` attribute compares each listed trait's final path
//! segment against the configured names. `#[derive(serde::Serialize)]`
//! matches when `"Serialize"` is configured — the final segment is what
//! the macro expands against.
//!
//! What's out of scope: non-derive attributes (`#[inline]`, `#[allow]`,
//! `#[must_use]`). Other rules cover those.

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

/// Find every `#[derive(Name)]` match in `ast`.
pub fn find_derive_matches(file: &str, ast: &syn::File, names: &[String]) -> Vec<MatchLocation> {
    let mut visitor = DeriveVisitor {
        file,
        names,
        hits: Vec::new(),
    };
    visitor.visit_file(ast);
    visitor.hits
}

struct DeriveVisitor<'a> {
    file: &'a str,
    names: &'a [String],
    hits: Vec<MatchLocation>,
}

impl DeriveVisitor<'_> {
    /// Inspect one item's attribute list and record any banned derives.
    /// Operation: iterator chain over attrs and their derive entries.
    fn inspect(&mut self, item_name: &str, attrs: &[syn::Attribute]) {
        attrs
            .iter()
            .filter(|a| a.path().is_ident("derive"))
            .for_each(|attr| self.scan_derive_list(item_name, attr));
    }

    /// Parse the `#[derive(...)]` token list and flag banned traits.
    /// Operation: comma-separated Path parser + per-path scan.
    fn scan_derive_list(&mut self, item_name: &str, attr: &syn::Attribute) {
        use syn::punctuated::Punctuated;
        let Ok(paths) =
            attr.parse_args_with(Punctuated::<syn::Path, syn::Token![,]>::parse_terminated)
        else {
            return;
        };
        paths
            .iter()
            .filter_map(|p| p.segments.last().map(|seg| (seg.ident.to_string(), seg)))
            .filter(|(name, _)| self.names.iter().any(|n| n == name))
            .for_each(|(name, seg)| {
                let start = seg.ident.span().start();
                self.hits.push(MatchLocation {
                    file: self.file.to_string(),
                    line: start.line,
                    column: start.column,
                    kind: ViolationKind::Derive {
                        trait_name: name,
                        item_name: item_name.to_string(),
                    },
                });
            });
    }
}

impl<'ast> Visit<'ast> for DeriveVisitor<'_> {
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let name = node.ident.to_string();
        self.inspect(&name, &node.attrs);
        visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let name = node.ident.to_string();
        self.inspect(&name, &node.attrs);
        visit::visit_item_enum(self, node);
    }

    fn visit_item_union(&mut self, node: &'ast syn::ItemUnion) {
        let name = node.ident.to_string();
        self.inspect(&name, &node.attrs);
        visit::visit_item_union(self, node);
    }
}
