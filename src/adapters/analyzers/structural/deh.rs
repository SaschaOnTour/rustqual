use syn::visit::Visit;

use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{has_cfg_test_attr, StructuralWarning, StructuralWarningKind};

/// Downcast method names that indicate broken polymorphism.
const DOWNCAST_METHODS: &[&str] = &["downcast_ref", "downcast_mut", "downcast"];

/// Detect downcast escape hatches: use of Any::downcast_*.
/// Operation: iterates parsed files, walks expressions for downcast calls.
pub(crate) fn detect_deh(
    warnings: &mut Vec<StructuralWarning>,
    parsed: &[(String, String, syn::File)],
    config: &StructuralConfig,
) {
    if !config.check_deh {
        return;
    }
    parsed.iter().for_each(|(path, _, syntax)| {
        let mut visitor = DowncastVisitor {
            file: path.clone(),
            warnings,
            in_test: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn detect_in(source: &str) -> Vec<StructuralWarning> {
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
        let config = StructuralConfig::default();
        let mut warnings = Vec::new();
        detect_deh(&mut warnings, &parsed, &config);
        warnings
    }

    #[test]
    fn test_downcast_ref_flagged() {
        let w = detect_in("fn foo(a: &dyn std::any::Any) { a.downcast_ref::<i32>(); }");
        assert_eq!(w.len(), 1);
        assert!(matches!(
            w[0].kind,
            StructuralWarningKind::DowncastEscapeHatch
        ));
    }

    #[test]
    fn test_downcast_mut_flagged() {
        let w = detect_in("fn foo(a: &mut dyn std::any::Any) { a.downcast_mut::<i32>(); }");
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn test_no_downcast_not_flagged() {
        let w = detect_in("fn foo() { let x = 42; }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_test_code_excluded() {
        let w = detect_in(
            "#[cfg(test)] mod tests { fn foo(a: &dyn std::any::Any) { a.downcast_ref::<i32>(); } }",
        );
        assert!(w.is_empty());
    }

    #[test]
    fn test_disabled_check() {
        let syntax = syn::parse_file("fn foo(a: &dyn std::any::Any) { a.downcast_ref::<i32>(); }")
            .expect("test source");
        let parsed = vec![("test.rs".to_string(), String::new(), syntax)];
        let config = StructuralConfig {
            check_deh: false,
            ..StructuralConfig::default()
        };
        let mut warnings = Vec::new();
        detect_deh(&mut warnings, &parsed, &config);
        assert!(warnings.is_empty());
    }
}
