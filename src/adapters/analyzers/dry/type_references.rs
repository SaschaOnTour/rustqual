//! Which names the code refers to — the evidence that keeps a declaration alive.
//!
//! Every identifier occurrence counts: type positions, expression paths,
//! patterns, generic bounds, derive names and macro token streams. That is the
//! same grain the call graph works at (a call is recorded by its last path
//! segment), and it is deliberately generous: over-collecting can only suppress
//! a finding, while under-collecting invents one — and telling an author to
//! delete a type that is in use is the expensive mistake.
//!
//! Exactly two things are *not* a reference: a declaration's own name, and the
//! self type of an `impl` block. Without the second, a type carrying only its
//! own methods would keep itself alive and nothing could ever be found.

use std::collections::HashSet;

use syn::visit::Visit;

use super::has_cfg_test;
use crate::adapters::shared::file_visitor::FileVisitor;
use crate::adapters::shared::macro_tokens;

/// AST visitor collecting referenced names, split by production / test context.
pub(crate) struct TypeReferenceCollector {
    pub(crate) production: HashSet<String>,
    pub(crate) tests: HashSet<String>,
    in_test: bool,
}

impl TypeReferenceCollector {
    pub(crate) fn new() -> Self {
        Self {
            production: HashSet::new(),
            tests: HashSet::new(),
            in_test: false,
        }
    }

    /// The reference set for the current context.
    /// Operation: one branch, no own calls.
    fn target(&mut self) -> &mut HashSet<String> {
        if self.in_test {
            &mut self.tests
        } else {
            &mut self.production
        }
    }

    /// Everything surrounding a declaration's own name: its attributes and
    /// generics. The name itself is skipped — a declaration is not a use of
    /// itself. Integration: two delegations.
    fn around(&mut self, attrs: &[syn::Attribute], generics: &syn::Generics) {
        attrs.iter().for_each(|a| self.visit_attribute(a));
        self.visit_generics(generics);
    }

    /// The self type of an `impl` block, whose name does not count as a use.
    /// Integration: shape dispatch.
    fn self_type(&mut self, ty: &syn::Type) {
        match ty {
            syn::Type::Path(tp) => self.self_type_path(&tp.path),
            other => self.visit_type(other),
        }
    }

    /// Skip only the final segment's name: the module prefix (`impl inner::Foo`)
    /// and every generic argument (`impl Wrapper<Inner>`) are real references,
    /// and dropping them would manufacture findings.
    /// Operation: positional walk, own calls hidden in the closure.
    fn self_type_path(&mut self, path: &syn::Path) {
        let last = path.segments.len().saturating_sub(1);
        path.segments.iter().enumerate().for_each(|(i, seg)| {
            if i != last {
                self.visit_ident(&seg.ident);
            }
            self.visit_path_arguments(&seg.arguments);
        });
    }
}

impl FileVisitor for TypeReferenceCollector {
    fn reset_for_file(&mut self, file_path: &str) {
        self.in_test = false;
        let _ = file_path;
    }
}

impl<'ast> Visit<'ast> for TypeReferenceCollector {
    fn visit_ident(&mut self, node: &'ast syn::Ident) {
        let name = node.to_string();
        self.target().insert(name);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.around(&node.attrs, &node.generics);
        self.visit_fields(&node.fields);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.around(&node.attrs, &node.generics);
        node.variants.iter().for_each(|v| self.visit_variant(v));
    }

    fn visit_item_union(&mut self, node: &'ast syn::ItemUnion) {
        self.around(&node.attrs, &node.generics);
        self.visit_fields_named(&node.fields);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.around(&node.attrs, &node.generics);
        self.visit_type(&node.ty);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.around(&node.attrs, &node.generics);
        self.visit_type(&node.ty);
        self.visit_expr(&node.expr);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        node.attrs.iter().for_each(|a| self.visit_attribute(a));
        self.visit_type(&node.ty);
        self.visit_expr(&node.expr);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let prev = self.in_test;
        if has_cfg_test(&node.attrs) {
            self.in_test = true;
        }
        self.around(&node.attrs, &node.generics);
        node.trait_
            .iter()
            .for_each(|(_, path, _)| self.visit_path(path));
        self.self_type(&node.self_ty);
        node.items.iter().for_each(|i| self.visit_impl_item(i));
        self.in_test = prev;
    }

    /// An attribute's arguments are an opaque token stream too, so
    /// `#[derive(Serialize)]` would otherwise not count as using `Serialize`.
    fn visit_meta_list(&mut self, node: &'ast syn::MetaList) {
        self.visit_path(&node.path);
        let idents: Vec<String> = macro_tokens::all_idents(&node.tokens).collect();
        self.target().extend(idents);
    }

    /// A macro body is an opaque token stream to `syn`, so every reference
    /// inside it would be invisible — the blind spot that would make
    /// macro-driven code look dead.
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.visit_path(&node.path);
        let idents: Vec<String> = macro_tokens::all_idents(&node.tokens).collect();
        self.target().extend(idents);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let prev = self.in_test;
        if has_cfg_test(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_test = prev;
    }
}

/// Collect referenced names across all parsed files.
/// Integration: per-file context switch + visitor delegation.
pub(crate) fn collect_type_references(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    let mut collector = TypeReferenceCollector::new();
    parsed.iter().for_each(|(path, _, file)| {
        collector.in_test = cfg_test_files.contains(path);
        syn::visit::visit_file(&mut collector, file);
    });
    (collector.production, collector.tests)
}
