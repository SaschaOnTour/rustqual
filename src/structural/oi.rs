use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralMetadata, StructuralWarning, StructuralWarningKind};

/// Detect orphaned impls: inherent impl in different file than type definition.
/// Operation: compares impl file against type definition file.
pub(crate) fn detect_oi(
    warnings: &mut Vec<StructuralWarning>,
    meta: &StructuralMetadata,
    config: &StructuralConfig,
) {
    if !config.check_oi {
        return;
    }
    meta.inherent_impls.iter().for_each(|(type_name, impl_file)| {
        if let Some(def_file) = meta.type_defs.get(type_name) {
            let def_module = top_level_module(def_file);
            let impl_module = top_level_module(impl_file);
            // Same top-level module is OK (e.g. analyzer/mod.rs + analyzer/types.rs)
            if def_module != impl_module {
                warnings.push(StructuralWarning {
                    file: impl_file.clone(),
                    line: 1,
                    name: type_name.clone(),
                    kind: StructuralWarningKind::OrphanedImpl {
                        defining_file: def_file.clone(),
                    },
                    dimension: Dimension::Coupling,
                    suppressed: false,
                });
            }
        }
    });
}

/// Extract the top-level module name from a file path.
/// Operation: string splitting.
fn top_level_module(path: &str) -> String {
    path.split('/').next().unwrap_or(path).replace(".rs", "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::collect_metadata;

    fn detect_multi(sources: &[(&str, &str)]) -> Vec<StructuralWarning> {
        let parsed: Vec<(String, String, syn::File)> = sources
            .iter()
            .map(|(path, src)| {
                let syntax = syn::parse_file(src).expect("test source");
                (path.to_string(), src.to_string(), syntax)
            })
            .collect();
        let meta = collect_metadata(&parsed);
        let config = StructuralConfig::default();
        let mut warnings = Vec::new();
        detect_oi(&mut warnings, &meta, &config);
        warnings
    }

    #[test]
    fn test_same_file_not_flagged() {
        let w = detect_multi(&[("lib.rs", "struct Foo {} impl Foo { fn bar() {} }")]);
        assert!(w.is_empty());
    }

    #[test]
    fn test_different_module_flagged() {
        let w = detect_multi(&[
            ("types.rs", "pub struct Foo {}"),
            ("other.rs", "impl Foo { fn bar() {} }"),
        ]);
        assert_eq!(w.len(), 1);
        assert!(matches!(w[0].kind, StructuralWarningKind::OrphanedImpl { .. }));
    }

    #[test]
    fn test_same_module_tree_not_flagged() {
        let w = detect_multi(&[
            ("analyzer/mod.rs", "pub struct Analyzer {}"),
            ("analyzer/helpers.rs", "impl Analyzer { fn helper() {} }"),
        ]);
        assert!(w.is_empty(), "same top-level module should not be flagged");
    }

    #[test]
    fn test_trait_impl_not_flagged() {
        // Trait impls are expected in separate files — collect_metadata only puts
        // inherent impls in inherent_impls, not trait impls
        let w = detect_multi(&[
            ("types.rs", "pub struct Foo {} pub trait Bar { fn baz(&self); }"),
            ("other.rs", "impl Bar for Foo { fn baz(&self) {} }"),
        ]);
        assert!(w.is_empty());
    }

    #[test]
    fn test_external_type_not_flagged() {
        // Type not defined in any parsed file
        let w = detect_multi(&[("other.rs", "impl ExternalType { fn bar() {} }")]);
        assert!(w.is_empty(), "external type should not be flagged");
    }

    #[test]
    fn test_disabled_check() {
        let parsed: Vec<(String, String, syn::File)> = vec![
            ("a.rs".to_string(), "pub struct Foo {}".to_string(), syn::parse_file("pub struct Foo {}").expect("test")),
            ("b.rs".to_string(), "impl Foo { fn bar() {} }".to_string(), syn::parse_file("impl Foo { fn bar() {} }").expect("test")),
        ];
        let meta = collect_metadata(&parsed);
        let config = StructuralConfig { check_oi: false, ..StructuralConfig::default() };
        let mut warnings = Vec::new();
        detect_oi(&mut warnings, &meta, &config);
        assert!(warnings.is_empty());
    }
}
