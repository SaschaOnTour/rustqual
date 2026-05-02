//! `pub use` tree walking — extracted from `pub_fns_visibility` so the
//! source/export canonical registration plus the workspace-type-only
//! filter stay together, off the visibility module's SRP budget.

use std::collections::HashSet;

use super::bindings::{canonicalise_type_segments_in_scope, CanonScope};
use super::local_symbols::FileScope;
use super::pub_fns_visibility::canonical_for_decl;

/// Bundled inputs for the `pub use`-tree walker. Carries the per-file
/// scope, current mod stack, and the workspace type-canonical filter.
pub(super) struct UseTreeCtx<'a> {
    pub file_scope: &'a FileScope<'a>,
    pub mod_stack: &'a [String],
    pub type_canonicals: &'a HashSet<String>,
}

/// Recursive walk over a `pub use` tree. For each leaf, register the
/// source-canonical (leaf path resolved through the workspace alias /
/// local-symbol pipeline) and, when that source resolves to a *type*
/// item, the export-canonical (current scope plus exported name).
///
/// Operation: closure-hidden descent into nested `Group`s and `Path`s.
// qual:recursive
pub(super) fn walk_use_tree(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    ctx: &UseTreeCtx<'_>,
    out: &mut HashSet<String>,
) {
    let recurse = |sub: &syn::UseTree, prefix: &mut Vec<String>, out: &mut HashSet<String>| {
        walk_use_tree(sub, prefix, ctx, out);
    };
    let add_export = |exported: &str, out: &mut HashSet<String>| {
        out.insert(canonical_for_decl(
            ctx.file_scope.path,
            ctx.mod_stack,
            exported,
        ));
    };
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            recurse(&p.tree, prefix, out);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let leaf = n.ident.to_string();
            prefix.push(leaf.clone());
            let is_type = resolve_use_source_type(prefix, ctx, out);
            prefix.pop();
            if is_type {
                add_export(&leaf, out);
            }
        }
        syn::UseTree::Rename(r) => {
            prefix.push(r.ident.to_string());
            let is_type = resolve_use_source_type(prefix, ctx, out);
            prefix.pop();
            if is_type {
                add_export(&r.rename.to_string(), out);
            }
        }
        syn::UseTree::Group(g) => {
            for sub in &g.items {
                recurse(sub, prefix, out);
            }
        }
        syn::UseTree::Glob(_) => {}
    }
}

/// Resolve a `pub use` leaf to its source canonical and verify the
/// resolved item is actually a *type* in the workspace, not a fn,
/// const, or static. `canonicalise_type_segments_in_scope` consults
/// `local_symbols` which covers all item kinds, so the resolver alone
/// would treat `pub use crate::other::helper as Foo;` (a value re-
/// export) as a type and pull a same-named private type's impl methods
/// into the public call-parity surface — verifying against the
/// workspace-type-canonical set is what blocks that.
fn resolve_use_source_type(
    segs: &[String],
    ctx: &UseTreeCtx<'_>,
    out: &mut HashSet<String>,
) -> bool {
    let scope = CanonScope {
        file: ctx.file_scope,
        mod_stack: ctx.mod_stack,
    };
    let Some(canonical) = canonicalise_type_segments_in_scope(segs, &scope) else {
        return false;
    };
    let joined = canonical.join("::");
    let is_type = ctx.type_canonicals.contains(&joined);
    if is_type {
        out.insert(joined);
    }
    is_type
}
