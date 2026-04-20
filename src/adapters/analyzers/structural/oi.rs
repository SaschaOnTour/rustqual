use crate::adapters::shared::file_to_module::file_to_module;
use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralMetadata, StructuralWarning, StructuralWarningKind};

/// Detect orphaned impls: inherent impl in a different **top-level
/// module** than the type definition (sibling files under the same
/// module — e.g. `analyzer/mod.rs` and `analyzer/types.rs` — are
/// intentionally allowed). Impls in entirely different modules are
/// flagged as "defined elsewhere".
/// Operation: compares top-level modules derived from the impl's and
/// type-def's file paths via `shared::file_to_module`.
pub(crate) fn detect_oi(
    warnings: &mut Vec<StructuralWarning>,
    meta: &StructuralMetadata,
    config: &StructuralConfig,
) {
    if !config.check_oi {
        return;
    }
    meta.inherent_impls
        .iter()
        .for_each(|(type_name, impl_file, impl_line)| {
            if let Some(def_file) = meta.type_defs.get(type_name) {
                let def_module = file_to_module(def_file);
                let impl_module = file_to_module(impl_file);
                // Same top-level module is OK (e.g. analyzer/mod.rs + analyzer/types.rs)
                if def_module != impl_module {
                    warnings.push(StructuralWarning {
                        file: impl_file.clone(),
                        line: *impl_line,
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
