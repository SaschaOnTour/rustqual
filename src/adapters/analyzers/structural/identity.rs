//! What a path in an `impl` header says about which declaration it means.
//!
//! The structural metadata knows types and traits by bare name, so two
//! declarations of one name are one entry. These helpers record the little
//! the syntax does say about an impl's trait — whether its path can only mean
//! a trait of its own module, whether a cfg gates it, and which names its
//! file imports — so SIT can count an impl only where it *certainly* means a
//! private local trait. Uncertainty only ever turns a verdict into *no finding*.

use syn::spanned::Spanned;

use super::StructuralMetadata;

/// Stand-in for "every name" a glob import may bring in.
pub(crate) const ANY_NAME: &str = "*";

/// The traits of the std prelude, 2021 and 2024 editions included: a bare
/// `impl Default for S` or `unsafe impl Send for S` implements the std trait
/// without any import in sight.
const PRELUDE_TRAITS: [&str; 31] = [
    "AsMut",
    "AsRef",
    "Clone",
    "Copy",
    "Default",
    "DoubleEndedIterator",
    "Drop",
    "Eq",
    "ExactSizeIterator",
    "Extend",
    "Fn",
    "FnMut",
    "FnOnce",
    "From",
    "FromIterator",
    "Into",
    "IntoIterator",
    "Iterator",
    "Ord",
    "PartialEq",
    "PartialOrd",
    "Send",
    "Sized",
    "Sync",
    "ToOwned",
    "ToString",
    "TryFrom",
    "TryInto",
    "Unpin",
    "Future",
    "IntoFuture",
];

/// Whether a trait path can only mean a trait declared in the impl's own
/// module: a bare name that is no prelude name. A qualified path — `self::`
/// and `super::` included — may reach a re-export of a foreign trait
/// (`self::facade::Display`), so it cannot be attributed by name.
/// Operation: one shape check, no own calls.
pub(crate) fn certain_trait_path(path: &syn::Path) -> bool {
    match path.segments.iter().collect::<Vec<_>>().as_slice() {
        [single] => !PRELUDE_TRAITS.contains(&single.ident.to_string().as_str()),
        _ => false,
    }
}

/// Whether an item carries a `#[cfg(…)]` or `#[cfg_attr(…)]` — a condition
/// rustqual does not evaluate, so the item may not exist.
/// Operation: attribute scan, no own calls.
pub(crate) fn has_cfg(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().is_ident("cfg") || a.path().is_ident("cfg_attr"))
}

/// Every name a `use` binds — the leaf, or only the alias under a rename —
/// plus `ANY_NAME` for a glob. Any root counts: `self::` and `super::` may
/// reach a re-export as well as `crate::` can.
/// Operation: one tree walk, then a projection, own call in the operand.
pub(crate) fn imported_names(item: &syn::ItemUse) -> Vec<String> {
    let mut bound = Vec::new();
    bindings(&item.tree, None, &mut bound);
    bound.into_iter().map(|(_, name)| name).collect()
}

/// (root segment, name bound in scope) for every leaf of a `use` tree; a
/// glob binds `ANY_NAME`.
/// Operation: recursive tree walk.
// qual:recursive
fn bindings(tree: &syn::UseTree, root: Option<&str>, out: &mut Vec<(String, String)>) {
    let root_or = |ident: &syn::Ident| {
        root.map(str::to_string)
            .unwrap_or_else(|| ident.to_string())
    };
    match tree {
        syn::UseTree::Path(p) => {
            let r = root_or(&p.ident);
            bindings(&p.tree, Some(&r), out);
        }
        syn::UseTree::Name(n) => out.push((root_or(&n.ident), n.ident.to_string())),
        syn::UseTree::Rename(r) => out.push((root_or(&r.ident), r.rename.to_string())),
        syn::UseTree::Glob(_) => out.extend(root.map(|r| (r.to_string(), ANY_NAME.to_string()))),
        syn::UseTree::Group(g) => g.items.iter().for_each(|t| bindings(t, root, out)),
    }
}

/// Record one production impl: a trait impl by the trait's bare name, with
/// whether its path can only mean a trait of the module chain; an inherent
/// impl with its line.
/// Operation: branch on the impl kind, own calls in the operands.
pub(crate) fn record_impl(imp: &syn::ItemImpl, path: &str, meta: &mut StructuralMetadata) {
    let Some(type_name) = super::extract_impl_type_name(imp) else {
        return;
    };
    let line = imp.span().start().line;
    match &imp.trait_ {
        Some((_, tp, _)) => {
            let tn = tp
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            let certain = certain_trait_path(tp) && !has_cfg(&imp.attrs) && meta.cfg_depth == 0;
            meta.trait_impls
                .entry(tn)
                .or_default()
                .push((type_name, path.to_string(), certain));
        }
        // An inherent impl behind a cfg may not exist: no OI verdict on it.
        None if has_cfg(&imp.attrs) || meta.cfg_depth > 0 => {}
        None => meta
            .inherent_impls
            .push((type_name, path.to_string(), line)),
    }
}

/// Record what a `use` binds: a glob marks its file, a name is kept with its
/// file.
/// Operation: one partition over the bound names, own call in the operand.
pub(crate) fn record_outside_imports(
    item: &syn::ItemUse,
    path: &str,
    meta: &mut StructuralMetadata,
) {
    imported_names(item)
        .into_iter()
        .for_each(|name| match name == ANY_NAME {
            true => {
                meta.glob_files.insert(path.to_string());
            }
            false => {
                meta.outside_imports.insert((path.to_string(), name));
            }
        });
}
