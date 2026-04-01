use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralMetadata, StructuralWarning, StructuralWarningKind};

/// Detect single-implementation traits: non-pub trait with exactly 1 impl.
/// Operation: compares trait definitions against impl counts.
pub(crate) fn detect_sit(
    warnings: &mut Vec<StructuralWarning>,
    meta: &StructuralMetadata,
    config: &StructuralConfig,
) {
    if !config.check_sit {
        return;
    }
    meta.trait_defs.iter().for_each(|(trait_name, info)| {
        // Skip pub traits (may have external impls)
        if info.is_pub {
            return;
        }
        // Skip marker traits (no methods)
        if info.method_count == 0 {
            return;
        }
        let impl_count = meta
            .trait_impls
            .get(trait_name)
            .map(|v| v.len())
            .unwrap_or(0);
        if impl_count == 1 {
            let (impl_type, _) = &meta.trait_impls[trait_name][0];
            warnings.push(StructuralWarning {
                file: info.file.clone(),
                line: 1,
                name: trait_name.clone(),
                kind: StructuralWarningKind::SingleImplTrait {
                    impl_type: impl_type.clone(),
                },
                dimension: Dimension::Srp,
                suppressed: false,
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::collect_metadata;

    fn detect_from(source: &str) -> Vec<StructuralWarning> {
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("lib.rs".to_string(), source.to_string(), syntax)];
        let meta = collect_metadata(&parsed);
        let config = StructuralConfig::default();
        let mut warnings = Vec::new();
        detect_sit(&mut warnings, &meta, &config);
        warnings
    }

    #[test]
    fn test_single_impl_flagged() {
        let w = detect_from(
            "trait Drawable { fn draw(&self); } struct Circle; impl Drawable for Circle { fn draw(&self) {} }",
        );
        assert_eq!(w.len(), 1);
        assert!(matches!(w[0].kind, StructuralWarningKind::SingleImplTrait { .. }));
        assert_eq!(w[0].name, "Drawable");
    }

    #[test]
    fn test_multiple_impls_not_flagged() {
        let w = detect_from(
            "trait Drawable { fn draw(&self); } struct Circle; struct Square; impl Drawable for Circle { fn draw(&self) {} } impl Drawable for Square { fn draw(&self) {} }",
        );
        assert!(w.is_empty());
    }

    #[test]
    fn test_pub_trait_excluded() {
        let w = detect_from(
            "pub trait Drawable { fn draw(&self); } struct Circle; impl Drawable for Circle { fn draw(&self) {} }",
        );
        assert!(w.is_empty());
    }

    #[test]
    fn test_marker_trait_excluded() {
        let w = detect_from(
            "trait Marker {} struct Circle; impl Marker for Circle {}",
        );
        assert!(w.is_empty());
    }

    #[test]
    fn test_zero_impls_not_flagged() {
        let w = detect_from("trait Drawable { fn draw(&self); }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_disabled_check() {
        let syntax = syn::parse_file("trait D { fn d(&self); } struct C; impl D for C { fn d(&self) {} }").expect("test source");
        let parsed = vec![("lib.rs".to_string(), String::new(), syntax)];
        let meta = collect_metadata(&parsed);
        let config = StructuralConfig { check_sit: false, ..StructuralConfig::default() };
        let mut warnings = Vec::new();
        detect_sit(&mut warnings, &meta, &config);
        assert!(warnings.is_empty());
    }
}
