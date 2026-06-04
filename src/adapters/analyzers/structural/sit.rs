use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralMetadata, StructuralWarning, StructuralWarningKind};

/// Detect single-implementation traits: non-pub trait with exactly 1 impl
/// total, counting `#[cfg(test)]` impls (a test double is a real implementor,
/// so a trait with prod impl + test doubles is a DI seam, not over-abstraction).
/// Operation: compares trait definitions against total impl counts.
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
        // Carry the Vec reference through so the single-impl case is
        // verified and unwrapped in one place. Avoids a separate
        // `[trait_name][0]` index that would panic if the collection
        // invariant ever drifted.
        let Some(impls) = meta.trait_impls.get(trait_name) else {
            return;
        };
        let [(impl_type, _)] = impls.as_slice() else {
            return;
        };
        // A `#[cfg(test)]` impl is a real implementor too: one prod impl plus any
        // test doubles is the idiomatic DI / test-seam pattern, not an
        // over-abstraction. Only fire when the TOTAL implementor count is 1.
        if meta
            .cfg_test_trait_impl_counts
            .get(trait_name)
            .copied()
            .unwrap_or(0)
            != 0
        {
            return;
        }
        warnings.push(StructuralWarning {
            file: info.file.clone(),
            line: info.line,
            name: trait_name.clone(),
            kind: StructuralWarningKind::SingleImplTrait {
                impl_type: impl_type.clone(),
            },
            dimension: Dimension::Coupling,
            suppressed: false,
        });
    });
}
