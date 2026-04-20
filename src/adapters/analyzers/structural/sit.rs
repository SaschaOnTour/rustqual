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
                line: info.line,
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
