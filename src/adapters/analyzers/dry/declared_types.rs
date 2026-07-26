//! Collecting the declarations DRY-006 judges: types proper (`struct`, `enum`,
//! `union`, `type`) plus `const` and `static`.
//!
//! Traits are deliberately absent. Every `impl Trait for X` names the trait, so
//! a trait with a single implementation would always look used — the check
//! would carry the full false-finding risk without the value. Functions belong
//! to DRY-002, and associated items (`impl Foo { const MAX … }`) are reachable
//! only through their type, which this check already judges.

use syn::visit::Visit;

use super::allow_scope::AllowScope;
use super::has_cfg_test;
use crate::adapters::shared::declared_type::{DeclaredType, TypeItemKind};
use crate::adapters::shared::file_visitor::FileVisitor;
use crate::adapters::shared::item_shape::{impl_item_attrs, item_attrs, trait_item_attrs};

/// AST visitor collecting type and constant declarations with their metadata.
pub(crate) struct DeclaredTypeCollector {
    pub(crate) file: String,
    pub(crate) types: Vec<DeclaredType>,
    in_test: bool,
    allow: AllowScope,
}

impl DeclaredTypeCollector {
    pub(crate) fn new() -> Self {
        Self {
            file: String::new(),
            types: Vec::new(),
            in_test: false,
            allow: AllowScope::default(),
        }
    }

    /// Record one declaration.
    /// Operation: struct construction, no own calls.
    fn record(&mut self, name: &syn::Ident, kind: TypeItemKind, attrs: &[syn::Attribute]) {
        self.types.push(DeclaredType {
            name: name.to_string(),
            kind,
            file: self.file.clone(),
            line: name.span().start().line,
            is_test: self.in_test || has_cfg_test(attrs),
            has_allow_dead_code: self.allow.covers(attrs),
            is_api: false,
            is_test_helper: false,
        });
    }
}

impl FileVisitor for DeclaredTypeCollector {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
        self.in_test = false;
    }
}

impl<'ast> Visit<'ast> for DeclaredTypeCollector {
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.record(&node.ident, TypeItemKind::Struct, &node.attrs);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.record(&node.ident, TypeItemKind::Enum, &node.attrs);
    }

    fn visit_item_union(&mut self, node: &'ast syn::ItemUnion) {
        self.record(&node.ident, TypeItemKind::Union, &node.attrs);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.record(&node.ident, TypeItemKind::TypeAlias, &node.attrs);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.record(&node.ident, TypeItemKind::Const, &node.attrs);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.record(&node.ident, TypeItemKind::Static, &node.attrs);
    }

    fn visit_file(&mut self, node: &'ast syn::File) {
        self.allow.enter_file(&node.attrs);
        syn::visit::visit_file(self, node);
    }

    /// Every item goes through here, so the two scope-forming attributes are
    /// handled once instead of on the handful of shapes that happen to need an
    /// override: `#[cfg(test)]` on a const or a `use` scopes exactly as it does
    /// on a function, and a lint level set on an `impl` or a function covers
    /// what is declared inside it.
    fn visit_item(&mut self, node: &'ast syn::Item) {
        let attrs = item_attrs(node);
        let prev_in_test = self.in_test;
        let prev_allow = self.allow.enter(attrs);
        self.in_test = prev_in_test || has_cfg_test(attrs);
        syn::visit::visit_item(self, node);
        self.allow.leave(prev_allow);
        self.in_test = prev_in_test;
    }

    /// Associated items do not pass through `visit_item`, so the same scoping
    /// happens at their own dispatch — a `#[cfg(test)]` associated const or
    /// type carries references just as a free one does.
    fn visit_impl_item(&mut self, node: &'ast syn::ImplItem) {
        let attrs = impl_item_attrs(node);
        let prev_in_test = self.in_test;
        let prev_allow = self.allow.enter(attrs);
        self.in_test = prev_in_test || has_cfg_test(attrs);
        syn::visit::visit_impl_item(self, node);
        self.allow.leave(prev_allow);
        self.in_test = prev_in_test;
    }

    fn visit_trait_item(&mut self, node: &'ast syn::TraitItem) {
        let attrs = trait_item_attrs(node);
        let prev_in_test = self.in_test;
        let prev_allow = self.allow.enter(attrs);
        self.in_test = prev_in_test || has_cfg_test(attrs);
        syn::visit::visit_trait_item(self, node);
        self.allow.leave(prev_allow);
        self.in_test = prev_in_test;
    }
}
