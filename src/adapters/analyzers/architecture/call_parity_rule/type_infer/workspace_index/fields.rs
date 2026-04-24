//! Struct-field-type collection.
//!
//! For every top-level `struct T { name: Type, … }` in the workspace,
//! record `(canonical_T, name) → CanonicalType(name's type)`. Tuple
//! structs, unit structs, and `Opaque` field types are skipped — they
//! contribute no value to later `self.field.method()` resolution.

use super::super::canonical::CanonicalType;
use super::super::resolve::resolve_type;
use super::{resolve_ctx_from_build, BuildContext, WorkspaceTypeIndex};
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::shared::cfg_test::has_cfg_test;
use syn::visit::Visit;

/// Walk `ast` and populate `index.struct_fields`. Uses `syn::visit::Visit`
/// so inline `#[cfg(test)]` modules are skipped but non-test inline mods
/// are traversed identically to the call-graph collector.
/// Integration: visitor delegates per-struct population.
pub(super) fn collect_from_file(
    index: &mut WorkspaceTypeIndex,
    ctx: &BuildContext<'_>,
    ast: &syn::File,
) {
    let mut collector = FieldCollector { index, ctx };
    collector.visit_file(ast);
}

struct FieldCollector<'i, 'c> {
    index: &'i mut WorkspaceTypeIndex,
    ctx: &'c BuildContext<'c>,
}

impl<'ast, 'i, 'c> Visit<'ast> for FieldCollector<'i, 'c> {
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        record_struct(self.index, self.ctx, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if has_cfg_test(&node.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }

    fn visit_item_impl(&mut self, _: &'ast syn::ItemImpl) {
        // Structs inside impl blocks don't exist syntactically — skip the
        // recursion so we don't waste walker cycles.
    }
}

/// Record every named field of `item`. Integration: canonicalisation +
/// per-field delegation.
fn record_struct(index: &mut WorkspaceTypeIndex, ctx: &BuildContext<'_>, item: &syn::ItemStruct) {
    let canon = |name: &str| canonical_struct_name(name, ctx);
    let canonical = canon(&item.ident.to_string());
    let syn::Fields::Named(named) = &item.fields else {
        return;
    };
    for field in &named.named {
        record_field(index, &canonical, ctx, field);
    }
}

/// Insert one `(struct, field) → type` entry, dropping `Opaque` types.
/// Operation. Own call to `resolve_type` hidden in closure for IOSP.
fn record_field(
    index: &mut WorkspaceTypeIndex,
    canonical: &str,
    ctx: &BuildContext<'_>,
    field: &syn::Field,
) {
    let resolve = |ty: &syn::Type| resolve_type(ty, &resolve_ctx_from_build(ctx));
    let Some(ident) = field.ident.as_ref() else {
        return;
    };
    let field_type = resolve(&field.ty);
    if matches!(field_type, CanonicalType::Opaque) {
        return;
    }
    index
        .struct_fields
        .insert((canonical.to_string(), ident.to_string()), field_type);
}

/// Build `crate::<file-module>::<StructIdent>` from a file path + ident.
/// Operation: pure string construction.
fn canonical_struct_name(struct_ident: &str, ctx: &BuildContext<'_>) -> String {
    let mut segs: Vec<String> = vec!["crate".to_string()];
    segs.extend(file_to_module_segments(ctx.path));
    segs.push(struct_ident.to_string());
    segs.join("::")
}
