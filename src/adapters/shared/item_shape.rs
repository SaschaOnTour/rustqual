//! What a `syn::Item` carries, whatever its kind: a name, a visibility, its
//! attributes.
//!
//! Several passes need one of these across all item kinds — the architecture
//! rule's local-symbol table, external reachability's `pub`-item set, the
//! dead-code collectors' lint and test scoping. Each needs a different slice,
//! but the *set of item shapes* is one fact, and a pass that forgot `union` or
//! `static` would silently under-report. It is spelled out once here.
//!
//! Attributes matter at this level in particular: `#[cfg(test)]` and
//! `#[allow(dead_code)]` are scope-forming on **every** item kind, so a visitor
//! that handles them only on the shapes it happens to override will get the
//! others wrong.

/// The declared identifier of an item, or `None` for shapes that declare no
/// name of their own (`impl`, `use`, `macro_rules!`, a foreign block, …).
/// Operation: shape lookup, no own calls.
pub(crate) fn item_ident(item: &syn::Item) -> Option<&syn::Ident> {
    item_ident_and_vis(item).map(|(ident, _)| ident)
}

/// The attributes of an item, whatever its kind. `syn` gives every variant its
/// own `attrs` field with no shared accessor, so the match lives here rather
/// than in each visitor that needs to scope on them.
/// Operation: shape lookup table, no own calls.
pub(crate) fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(i) => &i.attrs,
        syn::Item::Enum(i) => &i.attrs,
        syn::Item::ExternCrate(i) => &i.attrs,
        syn::Item::Fn(i) => &i.attrs,
        syn::Item::ForeignMod(i) => &i.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Macro(i) => &i.attrs,
        syn::Item::Mod(i) => &i.attrs,
        syn::Item::Static(i) => &i.attrs,
        syn::Item::Struct(i) => &i.attrs,
        syn::Item::Trait(i) => &i.attrs,
        syn::Item::TraitAlias(i) => &i.attrs,
        syn::Item::Type(i) => &i.attrs,
        syn::Item::Union(i) => &i.attrs,
        syn::Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

// `syn` models an `impl` block's and a trait's associated items as two separate
// enums with the same four named variants and no shared accessor, so the two
// look-ups differ in nothing but the type they match on. Rust cannot abstract
// that: a `path` fragment may not be extended with `::Variant` in pattern
// position, so a macro would have to take the variant list as an argument and
// the list would be written twice again. A trait would move the same two matches
// into two impls. Both are worse than saying so here.
// qual:allow(dry, duplicate) reason: "three syn enums with overlapping variant
// names and no shared accessor; every way to share the match is longer than the
// match, and they cannot drift because each is exhaustive over its own enum
// with a catch-all."
// qual:allow(dry, repeated_matches) reason: "same three enums — the repetition
// is syn's shape, not a missing abstraction."
/// The attributes of an associated item in an `impl` block.
/// Operation: shape lookup table, no own calls.
pub(crate) fn impl_item_attrs(item: &syn::ImplItem) -> &[syn::Attribute] {
    match item {
        syn::ImplItem::Const(i) => &i.attrs,
        syn::ImplItem::Fn(i) => &i.attrs,
        syn::ImplItem::Type(i) => &i.attrs,
        syn::ImplItem::Macro(i) => &i.attrs,
        _ => &[],
    }
}

/// The attributes of an associated item in a trait definition.
/// Operation: shape lookup table, no own calls.
pub(crate) fn trait_item_attrs(item: &syn::TraitItem) -> &[syn::Attribute] {
    match item {
        syn::TraitItem::Const(i) => &i.attrs,
        syn::TraitItem::Fn(i) => &i.attrs,
        syn::TraitItem::Type(i) => &i.attrs,
        syn::TraitItem::Macro(i) => &i.attrs,
        _ => &[],
    }
}

/// The attributes of an item in an `extern` block.
/// Operation: shape lookup table, no own calls.
pub(crate) fn foreign_item_attrs(item: &syn::ForeignItem) -> &[syn::Attribute] {
    match item {
        syn::ForeignItem::Fn(i) => &i.attrs,
        syn::ForeignItem::Static(i) => &i.attrs,
        syn::ForeignItem::Type(i) => &i.attrs,
        syn::ForeignItem::Macro(i) => &i.attrs,
        _ => &[],
    }
}

/// The declared identifier together with its visibility.
/// Operation: shape lookup table, no own calls.
pub(crate) fn item_ident_and_vis(item: &syn::Item) -> Option<(&syn::Ident, &syn::Visibility)> {
    match item {
        syn::Item::Fn(i) => Some((&i.sig.ident, &i.vis)),
        syn::Item::Mod(i) => Some((&i.ident, &i.vis)),
        syn::Item::Struct(i) => Some((&i.ident, &i.vis)),
        syn::Item::Enum(i) => Some((&i.ident, &i.vis)),
        syn::Item::Union(i) => Some((&i.ident, &i.vis)),
        syn::Item::Trait(i) => Some((&i.ident, &i.vis)),
        syn::Item::Type(i) => Some((&i.ident, &i.vis)),
        syn::Item::Const(i) => Some((&i.ident, &i.vis)),
        syn::Item::Static(i) => Some((&i.ident, &i.vis)),
        _ => None,
    }
}
