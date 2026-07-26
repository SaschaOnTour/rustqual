//! The name and visibility a top-level `syn::Item` declares.
//!
//! Several passes need "which items declare a name": the architecture rule's
//! local-symbol table, external reachability's `pub`-item set, and the
//! dead-type check's declaration collector. Each needs a different slice of it
//! — names only, names plus visibility, or per-kind metadata — but the *set of
//! item shapes that carry a name* is one fact, and a pass that forgot `union`
//! or `static` would silently under-report. It is spelled out once here.

/// The declared identifier of an item, or `None` for shapes that declare no
/// name of their own (`impl`, `use`, `macro_rules!`, a foreign block, …).
/// Operation: shape lookup, no own calls.
pub(crate) fn item_ident(item: &syn::Item) -> Option<&syn::Ident> {
    item_ident_and_vis(item).map(|(ident, _)| ident)
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
