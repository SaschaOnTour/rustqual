//! `forbid_item_kind` matcher — detects banned language-level item shapes.
//!
//! Each configured kind is a short identifier; the matcher walks the AST
//! once per file and emits one `MatchLocation` per occurrence. Supported
//! kinds:
//!
//!   - `async_fn` — any `async fn` (free, in impl, nested mod).
//!   - `unsafe_fn` — any `unsafe fn`.
//!   - `unsafe_impl` — any `unsafe impl Trait for Type`.
//!   - `static_mut` — any `static mut NAME: …`.
//!   - `extern_c_block` — any `extern "..." { … }` block (all ABIs).
//!   - `inline_cfg_test_module` — `#[cfg(test)] mod name { … }` with a body.
//!     Declaration-only form (`#[cfg(test)] mod name;`) is allowed.
//!   - `top_level_cfg_test_item` — any `#[cfg(test)]` item at the file's
//!     top level that isn't a `mod` (the `mod` case is covered by
//!     `inline_cfg_test_module`).
//!
//! The matcher is resilient to unknown kind strings: they are silently
//! ignored so a typo never crashes the run — callers should combine this
//! matcher with the strict `deny_unknown_fields` config validation.

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::HashSet;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

const KIND_ASYNC_FN: &str = "async_fn";
const KIND_UNSAFE_FN: &str = "unsafe_fn";
const KIND_UNSAFE_IMPL: &str = "unsafe_impl";
const KIND_STATIC_MUT: &str = "static_mut";
const KIND_EXTERN_C_BLOCK: &str = "extern_c_block";
const KIND_INLINE_CFG_TEST_MOD: &str = "inline_cfg_test_module";
const KIND_TOP_LEVEL_CFG_TEST_ITEM: &str = "top_level_cfg_test_item";

/// Find every occurrence of the requested item kinds in `ast`.
/// Integration: dispatches to per-family collection helpers.
pub fn find_item_kind_matches(file: &str, ast: &syn::File, kinds: &[String]) -> Vec<MatchLocation> {
    let requested: HashSet<&str> = kinds.iter().map(String::as_str).collect();
    let mut hits = Vec::new();
    collect_recursive(file, ast, &requested, &mut hits);
    collect_top_level_only(file, ast, &requested, &mut hits);
    hits
}

/// Collect kinds that can appear anywhere in the AST (impls, nested mods, …).
/// Operation: drives a syn::Visit walk.
fn collect_recursive(
    file: &str,
    ast: &syn::File,
    requested: &HashSet<&str>,
    hits: &mut Vec<MatchLocation>,
) {
    let mut visitor = RecursiveVisitor {
        file,
        requested,
        hits,
    };
    visitor.visit_file(ast);
}

/// Collect kinds that are only meaningful at the file's top level.
/// Operation: iterator chain over `ast.items`.
fn collect_top_level_only(
    file: &str,
    ast: &syn::File,
    requested: &HashSet<&str>,
    hits: &mut Vec<MatchLocation>,
) {
    ast.items
        .iter()
        .filter_map(|item| classify_top_level_item(item, requested))
        .for_each(|entry| hits.push(top_level_hit(file, entry)));
}

/// Classification result for one top-level item. The struct is owned
/// (no borrowed fields) — earlier revisions carried an unused `'a`
/// lifetime through a PhantomData marker; dropping it simplified the
/// signatures without any behavioural change.
struct TopLevelEntry {
    kind: &'static str,
    name: String,
    span: proc_macro2::Span,
}

/// Decide whether a top-level item falls under one of the top-level-only kinds.
/// Operation: pattern-match on Item enum.
fn classify_top_level_item(item: &syn::Item, requested: &HashSet<&str>) -> Option<TopLevelEntry> {
    match item {
        syn::Item::Mod(m) => classify_top_level_mod(m, requested),
        other => classify_top_level_non_mod(other, requested),
    }
}

/// Classify a top-level `mod` as inline_cfg_test_module when applicable.
/// Operation: inspect attrs + content.
fn classify_top_level_mod(m: &syn::ItemMod, requested: &HashSet<&str>) -> Option<TopLevelEntry> {
    if !requested.contains(KIND_INLINE_CFG_TEST_MOD) {
        return None;
    }
    if !has_cfg_test_attr(&m.attrs) || m.content.is_none() {
        return None;
    }
    Some(TopLevelEntry {
        kind: KIND_INLINE_CFG_TEST_MOD,
        name: m.ident.to_string(),
        span: m.ident.span(),
    })
}

/// Classify a non-mod top-level item as top_level_cfg_test_item when applicable.
/// Operation: inspect attrs of known item variants.
fn classify_top_level_non_mod(
    item: &syn::Item,
    requested: &HashSet<&str>,
) -> Option<TopLevelEntry> {
    if !requested.contains(KIND_TOP_LEVEL_CFG_TEST_ITEM) {
        return None;
    }
    let (attrs, name, span) = match item {
        syn::Item::Fn(i) => (&i.attrs, i.sig.ident.to_string(), i.sig.ident.span()),
        syn::Item::Impl(i) => (&i.attrs, String::new(), i.impl_token.span),
        syn::Item::Const(i) => (&i.attrs, i.ident.to_string(), i.ident.span()),
        syn::Item::Static(i) => (&i.attrs, i.ident.to_string(), i.ident.span()),
        syn::Item::Struct(i) => (&i.attrs, i.ident.to_string(), i.ident.span()),
        syn::Item::Enum(i) => (&i.attrs, i.ident.to_string(), i.ident.span()),
        syn::Item::Trait(i) => (&i.attrs, i.ident.to_string(), i.ident.span()),
        syn::Item::Type(i) => (&i.attrs, i.ident.to_string(), i.ident.span()),
        syn::Item::Use(i) => (&i.attrs, String::new(), i.use_token.span),
        _ => return None,
    };
    if !has_cfg_test_attr(attrs) {
        return None;
    }
    Some(TopLevelEntry {
        kind: KIND_TOP_LEVEL_CFG_TEST_ITEM,
        name,
        span,
    })
}

/// Turn a `TopLevelEntry` into a reportable `MatchLocation`.
/// Operation: field copy + span decomposition.
fn top_level_hit(file: &str, entry: TopLevelEntry) -> MatchLocation {
    let start = entry.span.start();
    MatchLocation {
        file: file.to_string(),
        line: start.line,
        column: start.column,
        kind: ViolationKind::ItemKind {
            kind: entry.kind,
            name: entry.name,
        },
    }
}

/// Check if any of `attrs` is `#[cfg(test)]`.
/// Operation: iterator scan over attributes.
fn has_cfg_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(is_cfg_test)
}

/// True when an attribute is literally `#[cfg(test)]`.
/// Operation: pattern-match on Attribute shape.
fn is_cfg_test(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("cfg") {
        return false;
    }
    attr.parse_args::<syn::Path>()
        .map(|p| p.is_ident("test"))
        .unwrap_or(false)
}

// ── Recursive visitor (async/unsafe fn, unsafe impl, static mut, extern) ──

struct RecursiveVisitor<'a> {
    file: &'a str,
    requested: &'a HashSet<&'a str>,
    hits: &'a mut Vec<MatchLocation>,
}

impl RecursiveVisitor<'_> {
    fn record(&mut self, kind: &'static str, name: String, span: proc_macro2::Span) {
        let start = span.start();
        self.hits.push(MatchLocation {
            file: self.file.to_string(),
            line: start.line,
            column: start.column,
            kind: ViolationKind::ItemKind { kind, name },
        });
    }
}

impl<'ast> Visit<'ast> for RecursiveVisitor<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        inspect_fn_signature(self, &node.sig);
        visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        inspect_fn_signature(self, &node.sig);
        visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if node.unsafety.is_some() && self.requested.contains(KIND_UNSAFE_IMPL) {
            self.record(KIND_UNSAFE_IMPL, String::new(), node.impl_token.span);
        }
        visit::visit_item_impl(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        if matches!(node.mutability, syn::StaticMutability::Mut(_))
            && self.requested.contains(KIND_STATIC_MUT)
        {
            self.record(KIND_STATIC_MUT, node.ident.to_string(), node.ident.span());
        }
        visit::visit_item_static(self, node);
    }

    fn visit_item_foreign_mod(&mut self, node: &'ast syn::ItemForeignMod) {
        if self.requested.contains(KIND_EXTERN_C_BLOCK) {
            self.record(
                KIND_EXTERN_C_BLOCK,
                String::new(),
                node.abi.extern_token.span,
            );
        }
        visit::visit_item_foreign_mod(self, node);
    }
}

/// Detect async/unsafe markers on a function signature.
/// Operation: flag checks on sig fields.
fn inspect_fn_signature(visitor: &mut RecursiveVisitor<'_>, sig: &syn::Signature) {
    if sig.asyncness.is_some() && visitor.requested.contains(KIND_ASYNC_FN) {
        visitor.record(KIND_ASYNC_FN, sig.ident.to_string(), sig.ident.span());
    }
    if sig.unsafety.is_some() && visitor.requested.contains(KIND_UNSAFE_FN) {
        visitor.record(KIND_UNSAFE_FN, sig.ident.to_string(), sig.ident.span());
    }
}
