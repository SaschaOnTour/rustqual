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

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let prev_in_test = self.in_test;
        let prev_allow = self.allow.enter(&node.attrs);
        if has_cfg_test(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.allow.leave(prev_allow);
        self.in_test = prev_in_test;
    }
}
