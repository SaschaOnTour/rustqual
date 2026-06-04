use std::collections::HashSet;

use syn::visit::Visit;

use crate::config::StructuralConfig;
use crate::findings::Dimension;

use crate::adapters::shared::cfg_test::has_test_attr;

use super::{has_cfg_test_attr, StructuralWarning, StructuralWarningKind};

/// Downcast method names that indicate broken polymorphism.
const DOWNCAST_METHODS: &[&str] = &["downcast_ref", "downcast_mut", "downcast"];

/// Detect downcast escape hatches: use of Any::downcast_*.
/// Test code is skipped: a file in `cfg_test_files` (integration-test dir,
/// `#![cfg(test)]`, or `#[cfg(test)] mod` chain) starts `in_test = true`;
/// inline `#[cfg(test)] mod` blocks and a function's own `#[test]` /
/// `#[cfg(test)]` attributes are handled per-item (the latter covers the
/// synthetic `#[test]` fns the macro-expansion pre-pass emits for
/// `quickcheck!` / `proptest!` properties, whose original `#[cfg(test)]`
/// wrapper is gone).
/// Operation: iterates parsed files, walks expressions for downcast calls.
pub(crate) fn detect_deh(
    warnings: &mut Vec<StructuralWarning>,
    parsed: &[(String, String, syn::File)],
    config: &StructuralConfig,
    cfg_test_files: &HashSet<String>,
) {
    if !config.check_deh {
        return;
    }
    parsed.iter().for_each(|(path, _, syntax)| {
        let mut visitor = DowncastVisitor {
            file: path.clone(),
            warnings,
            in_test: cfg_test_files.contains(path),
        };
        visitor.visit_file(syntax);
    });
}

/// Visitor that detects downcast method calls.
struct DowncastVisitor<'a> {
    file: String,
    warnings: &'a mut Vec<StructuralWarning>,
    in_test: bool,
}

impl<'ast, 'a> Visit<'ast> for DowncastVisitor<'a> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let was_in_test = self.in_test;
        if has_cfg_test_attr(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_test = was_in_test;
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let was_in_test = self.in_test;
        if has_test_attr(&node.attrs) || has_cfg_test_attr(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_fn(self, node);
        self.in_test = was_in_test;
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let was_in_test = self.in_test;
        if has_cfg_test_attr(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_impl(self, node);
        self.in_test = was_in_test;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let was_in_test = self.in_test;
        if has_cfg_test_attr(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_impl_item_fn(self, node);
        self.in_test = was_in_test;
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if !self.in_test {
            let method_name = node.method.to_string();
            if DOWNCAST_METHODS.contains(&method_name.as_str()) {
                let line = node.method.span().start().line;
                self.warnings.push(StructuralWarning {
                    file: self.file.clone(),
                    line,
                    name: method_name,
                    kind: StructuralWarningKind::DowncastEscapeHatch,
                    dimension: Dimension::Coupling,
                    suppressed: false,
                });
            }
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}
