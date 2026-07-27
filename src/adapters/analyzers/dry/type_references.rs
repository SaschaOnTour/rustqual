//! Which names the code refers to — the evidence that keeps a declaration alive.
//!
//! Every identifier occurrence counts: type positions, expression paths,
//! patterns, generic bounds, derive names and macro token streams. That is the
//! same grain the call graph works at (a call is recorded by its last path
//! segment), and it is deliberately generous: over-collecting can only suppress
//! a finding, while under-collecting invents one — and telling an author to
//! delete a type that is in use is the expensive mistake.
//!
//! Two positions are not recorded at all: a declaration's own name, and the
//! self type of an `impl` block. Without the second, a type carrying only its
//! own methods would keep itself alive and nothing could ever be found.
//!
//! Everything else is attributed to the declaration whose body made it, so that
//! `liveness` can ask what the roots actually reach rather than what is merely
//! mentioned. A reference from anything that is not a candidate — a function
//! body, a trait, a `use` — is rooted. A declaration naming itself in its own
//! body is recorded, as an edge to itself: harmless for reachability, since it
//! can only matter once the declaration is already alive, and
//! `ReferenceGraph::flatten` drops it for the marker check, which asks whether
//! somebody *else* names the declaration.

use std::collections::HashSet;

use syn::visit::Visit;

use super::doc_scan::{DocLine, DocScanner};
use super::liveness::ReferenceGraph;
use super::split_names::{collect_split, test_scoped_visits, SplitCollector, SplitNames};
use crate::adapters::shared::macro_tokens;

/// AST visitor collecting referenced names, split by production / test context
/// and attributed to the declaration that made them.
#[derive(Default)]
pub(crate) struct TypeReferenceCollector {
    names: SplitNames,
    docs: DocScanner,
    graph: ReferenceGraph,
    /// The declaration currently being walked, if any.
    owner: Option<String>,
}

impl SplitCollector for TypeReferenceCollector {
    fn names(&mut self) -> &mut SplitNames {
        &mut self.names
    }
}

impl TypeReferenceCollector {
    /// Record references attributed to the declaration being walked.
    /// Operation: field split + iteration, own calls hidden in the closure.
    fn record_in(&mut self, in_test: bool, names: impl IntoIterator<Item = String>) {
        names
            .into_iter()
            .for_each(|n| self.graph.add(self.owner.as_deref(), in_test, n));
    }

    /// Record one reference in the current context.
    /// Trivial: delegates with a single name.
    fn record(&mut self, name: String) {
        self.record_all(Some(name));
    }

    /// Record several references in the current context.
    /// Trivial: delegates with the current context.
    fn record_all(&mut self, names: impl IntoIterator<Item = String>) {
        self.record_in(self.names.in_test, names);
    }

    /// Record what a doc example named — test context whatever the surrounding
    /// item is, because the example is code `cargo test` compiles and runs.
    /// Trivial: delegates with the context forced.
    fn record_all_as_test(&mut self, names: impl IntoIterator<Item = String>) {
        self.record_in(true, names);
    }

    /// One line of a doc comment. A doc example is test code even on a
    /// production item; an intra-doc link documents the API, so it stays a
    /// production reference.
    /// Integration: scanner dispatch into the matching set.
    fn absorb_doc_line(&mut self, text: &str) {
        match self.docs.line(text) {
            DocLine::Fence => {}
            DocLine::Example(names) => self.record_all_as_test(names),
            DocLine::Prose(names) => self.record_all(names),
        }
    }

    /// Everything surrounding a declaration's own name: its attributes and
    /// generics. The name itself is skipped — a declaration is not a use of
    /// itself. Integration: two delegations.
    fn around(&mut self, attrs: &[syn::Attribute], generics: &syn::Generics) {
        attrs.iter().for_each(|a| self.visit_attribute(a));
        self.visit_generics(generics);
    }

    /// Enter a declaration: everything named until the matching `leave_owner`
    /// belongs to it. An `impl` block on a shape that is not a path
    /// (`impl Trait for [u8; 4]`) names no candidate, so it keeps whatever owner
    /// surrounds it. Returns the owner to restore, mirroring
    /// `SplitNames::enter`.
    /// Operation: save + replace, no own calls.
    fn enter_owner(&mut self, owner: Option<String>) -> Option<String> {
        let previous = self.owner.take();
        self.owner = owner.or_else(|| previous.clone());
        previous
    }

    /// Operation: field restore, no own calls.
    fn leave_owner(&mut self, previous: Option<String>) {
        self.owner = previous;
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
            let prefix = (i != last).then(|| seg.ident.to_string());
            self.record_all(prefix);
            self.visit_path_arguments(&seg.arguments);
        });
    }
}

/// The declaration an `impl` block hangs its items on — everything inside it is
/// only compiled because that type is. `None` for a shape that names no
/// declaration: a tuple or reference type, and a blanket impl, whose self type
/// is the block's own type parameter. `impl<Item> Ext for Item` must not be
/// attributed to a `struct Item` that happens to exist elsewhere — a parameter
/// shadowing a real name would tie the body to a verdict about the wrong thing.
/// Operation: two delegations to predicates, own calls hidden in the closure.
fn impl_owner(node: &syn::ItemImpl) -> Option<String> {
    local_path_tail(&node.self_ty).filter(|name| !is_type_param(&node.generics, name))
}

/// The last segment of a self type that certainly names a declaration of *this*
/// crate: a bare name, or one reached through `crate::` / `self::` / `super::`.
///
/// Anything else is left unowned. An extension trait on a foreign type is
/// ordinary Rust, and `impl Ext for other_crate::Entry` shares its last segment
/// with any local `Entry` — attributing the body to that one made a plainly
/// used type deletable as soon as the local `Entry` was dead. The same
/// conflation the SRP owner key already refuses to make for absolute paths; the
/// price is a missed finding on an `impl inner::Thing` body, which is the
/// direction this check errs in everywhere else too. A qualified self type
/// (`<Foo as Bar>::Assoc`) names an associated item, not a declaration, so it
/// is unowned as well.
/// Operation: path shape checks, no own calls.
fn local_path_tail(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(tp) = ty else {
        return None;
    };
    let first = tp.path.segments.first()?.ident.to_string();
    let names_this_crate =
        tp.path.segments.len() == 1 || matches!(first.as_str(), "crate" | "self" | "super");
    let unambiguous = tp.qself.is_none() && tp.path.leading_colon.is_none() && names_this_crate;
    unambiguous
        .then(|| tp.path.segments.last())
        .flatten()
        .map(|s| s.ident.to_string())
}

/// Whether `name` is one of this block's own type parameters.
/// Operation: predicate over the parameter list, no own calls.
fn is_type_param(generics: &syn::Generics, name: &str) -> bool {
    generics
        .params
        .iter()
        .any(|p| matches!(p, syn::GenericParam::Type(t) if t.ident == name))
}

impl<'ast> Visit<'ast> for TypeReferenceCollector {
    fn visit_ident(&mut self, node: &'ast syn::Ident) {
        self.record(node.to_string());
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let previous = self.enter_owner(Some(node.ident.to_string()));
        self.around(&node.attrs, &node.generics);
        self.visit_fields(&node.fields);
        self.leave_owner(previous);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        let previous = self.enter_owner(Some(node.ident.to_string()));
        self.around(&node.attrs, &node.generics);
        node.variants.iter().for_each(|v| self.visit_variant(v));
        self.leave_owner(previous);
    }

    fn visit_item_union(&mut self, node: &'ast syn::ItemUnion) {
        let previous = self.enter_owner(Some(node.ident.to_string()));
        self.around(&node.attrs, &node.generics);
        self.visit_fields_named(&node.fields);
        self.leave_owner(previous);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        let previous = self.enter_owner(Some(node.ident.to_string()));
        self.around(&node.attrs, &node.generics);
        self.visit_type(&node.ty);
        self.leave_owner(previous);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        let previous = self.enter_owner(Some(node.ident.to_string()));
        self.around(&node.attrs, &node.generics);
        self.visit_type(&node.ty);
        self.visit_expr(&node.expr);
        self.leave_owner(previous);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        let previous = self.enter_owner(Some(node.ident.to_string()));
        node.attrs.iter().for_each(|a| self.visit_attribute(a));
        self.visit_type(&node.ty);
        self.visit_expr(&node.expr);
        self.leave_owner(previous);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let previous = self.enter_owner(impl_owner(node));
        self.around(&node.attrs, &node.generics);
        node.trait_
            .iter()
            .for_each(|(_, path, _)| self.visit_path(path));
        self.self_type(&node.self_ty);
        node.items.iter().for_each(|i| self.visit_impl_item(i));
        self.leave_owner(previous);
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
        self.record_all(idents);
    }

    /// A macro body is an opaque token stream to `syn`, so every reference
    /// inside it would be invisible — the blind spot that would make
    /// macro-driven code look dead.
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.visit_path(&node.path);
        let idents: Vec<String> = macro_tokens::all_idents(&node.tokens).collect();
        self.record_all(idents);
    }

    test_scoped_visits!();

    /// The one attributed kind the shared list leaves out, because the call
    /// graph needs extra work here and a macro would hide it.
    fn visit_field(&mut self, node: &'ast syn::Field) {
        let previous = self.names.enter(&node.attrs);
        syn::visit::visit_field(self, node);
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

/// Collect the reference graph across all parsed files.
///
/// The driver's own `SplitNames` return value is empty by construction here —
/// this collector routes every name into the graph so it keeps the attribution;
/// what it takes from the shared driver is the per-file test context and the
/// `#[cfg(test)]` scoping, which is the part that must not exist twice.
/// Operation: drive the walk, then take the graph.
pub(crate) fn collect_reference_graph(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &HashSet<String>,
) -> ReferenceGraph {
    let mut collector = TypeReferenceCollector::default();
    let _ = collect_split(parsed, cfg_test_files, &mut collector);
    collector.graph
}
