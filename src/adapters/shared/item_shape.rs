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
