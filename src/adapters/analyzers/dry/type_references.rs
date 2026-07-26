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

use super::split_names::{collect_split, SplitCollector, SplitNames};
use crate::adapters::shared::macro_tokens;
use crate::adapters::shared::text_names::{code_names, doc_link_names, is_doc_fence};

/// AST visitor collecting referenced names, split by production / test context.
#[derive(Default)]
pub(crate) struct TypeReferenceCollector {
    names: SplitNames,
    /// Whether the doc lines currently arriving are inside a ``` fence. A doc
    /// comment reaches `syn` as one `#[doc = "…"]` attribute *per line*, so the
    /// fence can only be tracked across them. An unclosed fence would leak into
    /// the next item — over-collection, and broken docs to begin with.
    in_doc_fence: bool,
}

impl SplitCollector for TypeReferenceCollector {
    fn names(&mut self) -> &mut SplitNames {
        &mut self.names
    }
}

impl TypeReferenceCollector {
    /// The reference set for the current context.
    /// Trivial: delegates to the shared split.
    fn target(&mut self) -> &mut HashSet<String> {
        self.names.target()
    }

    /// One line of a doc comment: a fence marker flips the mode, code inside a
    /// fence contributes every name, prose contributes only its link targets.
    /// Operation: mode dispatch, own calls hidden in the closures.
    fn absorb_doc_line(&mut self, text: &str) {
        if is_doc_fence(text) {
            self.in_doc_fence = !self.in_doc_fence;
            return;
        }
        let names = match self.in_doc_fence {
            true => code_names(text),
            false => doc_link_names(text),
        };
        self.target().extend(names);
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
        let previous = self.names.enter(&node.attrs);
        self.around(&node.attrs, &node.generics);
        node.trait_
            .iter()
            .for_each(|(_, path, _)| self.visit_path(path));
        self.self_type(&node.self_ty);
        node.items.iter().for_each(|i| self.visit_impl_item(i));
        self.names.leave(previous);
    }

    /// A doc comment is `#[doc = "…"]` — one string per line, so what it names
    /// is invisible to any walk over tokens or the AST. Two things in there are
    /// real references: an intra-doc link, whose target must exist for the docs
    /// to build, and the body of a ``` fence, which is code `cargo test`
    /// compiles and runs. Prose is neither.
    fn visit_meta_name_value(&mut self, node: &'ast syn::MetaNameValue) {
        // The value is walked as syn would — overriding this method must not
        // quietly drop `#[foo = Bar]` or `#[doc = include_str!(…)]`, which
        // would be under-collection, the direction that invents findings.
        syn::visit::visit_meta_name_value(self, node);
        let line = doc_text(node);
        line.into_iter()
            .for_each(|text| self.absorb_doc_line(&text));
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
        let previous = self.names.enter(&node.attrs);
        syn::visit::visit_item_mod(self, node);
        self.names.leave(previous);
    }
}

/// The text of a `#[doc = "…"]` attribute; `None` for any other name-value
/// attribute.
/// Operation: shape match, no own calls.
fn doc_text(node: &syn::MetaNameValue) -> Option<String> {
    let is_doc = node.path.is_ident("doc");
    match (&node.value, is_doc) {
        (syn::Expr::Lit(lit), true) => match &lit.lit {
            syn::Lit::Str(s) => Some(s.value()),
            _ => None,
        },
        _ => None,
    }
}

/// Collect referenced names across all parsed files.
/// Trivial: delegates to the shared split driver.
pub(crate) fn collect_type_references(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &HashSet<String>,
) -> (HashSet<String>, HashSet<String>) {
    collect_split(
        parsed,
        cfg_test_files,
        &mut TypeReferenceCollector::default(),
    )
}
