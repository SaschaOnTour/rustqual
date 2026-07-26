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

/// The attributes of an expression.
///
/// `syn` gives every variant its own `attrs` field and no shared accessor
/// (`Expr::replace_attrs` is crate-private), so reaching them means naming the
/// variants. It is worth the match: an attributed expression statement —
/// `#[cfg(test)] consume(Fixture);` — is absent from a non-test build, and
/// counting its call as production does not merely miss a finding, it lets the
/// marker check report a working `qual:api` as spent.
///
/// `Expr` is `#[non_exhaustive]`; a variant added by a future `syn` falls into
/// the catch-all and is treated as carrying no attributes, which is the
/// direction that reports rather than suppresses.
/// Operation: shape lookup table, no own calls.
pub(crate) fn expr_attrs(expr: &syn::Expr) -> &[syn::Attribute] {
    match expr {
        syn::Expr::Array(e) => &e.attrs,
        syn::Expr::Assign(e) => &e.attrs,
        syn::Expr::Async(e) => &e.attrs,
        syn::Expr::Await(e) => &e.attrs,
        syn::Expr::Binary(e) => &e.attrs,
        syn::Expr::Block(e) => &e.attrs,
        syn::Expr::Break(e) => &e.attrs,
        syn::Expr::Call(e) => &e.attrs,
        syn::Expr::Cast(e) => &e.attrs,
        syn::Expr::Closure(e) => &e.attrs,
        syn::Expr::Const(e) => &e.attrs,
        syn::Expr::Continue(e) => &e.attrs,
        syn::Expr::Field(e) => &e.attrs,
        syn::Expr::ForLoop(e) => &e.attrs,
        syn::Expr::Group(e) => &e.attrs,
        syn::Expr::If(e) => &e.attrs,
        syn::Expr::Index(e) => &e.attrs,
        syn::Expr::Infer(e) => &e.attrs,
        syn::Expr::Let(e) => &e.attrs,
        syn::Expr::Lit(e) => &e.attrs,
        syn::Expr::Loop(e) => &e.attrs,
        syn::Expr::Macro(e) => &e.attrs,
        syn::Expr::Match(e) => &e.attrs,
        syn::Expr::MethodCall(e) => &e.attrs,
        syn::Expr::Paren(e) => &e.attrs,
        syn::Expr::Path(e) => &e.attrs,
        syn::Expr::Range(e) => &e.attrs,
        syn::Expr::RawAddr(e) => &e.attrs,
        syn::Expr::Reference(e) => &e.attrs,
        syn::Expr::Repeat(e) => &e.attrs,
        syn::Expr::Return(e) => &e.attrs,
        syn::Expr::Struct(e) => &e.attrs,
        syn::Expr::Try(e) => &e.attrs,
        syn::Expr::TryBlock(e) => &e.attrs,
        syn::Expr::Tuple(e) => &e.attrs,
        syn::Expr::Unary(e) => &e.attrs,
        syn::Expr::Unsafe(e) => &e.attrs,
        syn::Expr::While(e) => &e.attrs,
        syn::Expr::Yield(e) => &e.attrs,
        _ => &[],
    }
}

/// The attributes of a generic parameter. `#[cfg(test)]` is valid on all three
/// kinds and removes the parameter — a type parameter's default can name a type
/// nothing else mentions.
/// Operation: shape lookup table, no own calls.
pub(crate) fn generic_param_attrs(param: &syn::GenericParam) -> &[syn::Attribute] {
    match param {
        syn::GenericParam::Lifetime(p) => &p.attrs,
        syn::GenericParam::Type(p) => &p.attrs,
        syn::GenericParam::Const(p) => &p.attrs,
    }
}

/// The attributes of a pattern. Same story as [`expr_attrs`]: every variant
/// owns its `attrs` and `syn` offers no shared accessor.
///
/// `Pat` is `#[non_exhaustive]`; an unknown variant is treated as carrying no
/// attributes, the direction that reports rather than suppresses.
/// Operation: shape lookup table, no own calls.
pub(crate) fn pat_attrs(pat: &syn::Pat) -> &[syn::Attribute] {
    match pat {
        syn::Pat::Const(p) => &p.attrs,
        syn::Pat::Ident(p) => &p.attrs,
        syn::Pat::Lit(p) => &p.attrs,
        syn::Pat::Macro(p) => &p.attrs,
        syn::Pat::Or(p) => &p.attrs,
        syn::Pat::Paren(p) => &p.attrs,
        syn::Pat::Path(p) => &p.attrs,
        syn::Pat::Range(p) => &p.attrs,
        syn::Pat::Reference(p) => &p.attrs,
        syn::Pat::Rest(p) => &p.attrs,
        syn::Pat::Slice(p) => &p.attrs,
        syn::Pat::Struct(p) => &p.attrs,
        syn::Pat::Tuple(p) => &p.attrs,
        syn::Pat::TupleStruct(p) => &p.attrs,
        syn::Pat::Type(p) => &p.attrs,
        syn::Pat::Wild(p) => &p.attrs,
        _ => &[],
    }
}

/// The attribute a statement carries when it is an expression. `syn` binds it
/// to the **first operand** of an assignment or a binary expression rather than
/// to the expression itself (`#[cfg(test)] x = 1;` puts it on the path `x`), so
/// recovering what rustc sees as the statement's attribute means following that
/// edge. Rust has no way to attribute an operand on its own, so descending
/// cannot pick up something that was not the statement's.
/// Operation: leftmost-operand walk, own call in the arms.
// qual:recursive
fn statement_expr_attrs(expr: &syn::Expr) -> &[syn::Attribute] {
    match expr {
        syn::Expr::Assign(e) if e.attrs.is_empty() => statement_expr_attrs(&e.left),
        syn::Expr::Binary(e) if e.attrs.is_empty() => statement_expr_attrs(&e.left),
        other => expr_attrs(other),
    }
}

/// The attributes of a statement. A `#[cfg(test)]` here removes the statement
/// from a non-test build, so it scopes just like an item does — the enclosing
/// function being production says nothing about it.
///
/// `Stmt::Item` yields nothing on purpose: the item dispatch scopes it, and
/// counting the attributes twice would be harmless but confusing.
/// Trivial: shape dispatch, expression attributes delegated.
pub(crate) fn stmt_attrs(stmt: &syn::Stmt) -> &[syn::Attribute] {
    match stmt {
        syn::Stmt::Local(s) => &s.attrs,
        syn::Stmt::Macro(s) => &s.attrs,
        syn::Stmt::Expr(e, _) => statement_expr_attrs(e),
        syn::Stmt::Item(_) => &[],
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
