use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralMetadata, StructuralWarning, StructuralWarningKind};

/// Detect orphaned impls: inherent impl in a different **top-level
/// module** than the type definition (sibling files under the same
/// module — e.g. `analyzer/mod.rs` and `analyzer/types.rs` — are
/// intentionally allowed). Impls in entirely different modules are
/// flagged as "defined elsewhere".
/// Operation: compares top-level modules derived from the impl's and
/// type-def's file paths via `coupling::file_to_module`.
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
                let def_module = top_level_module(def_file);
                let impl_module = top_level_module(impl_file);
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

/// Extract the top-level module name from a file path. Accepts paths
/// both with and without the `src/` prefix: `src/foo/bar.rs` → `"foo"`,
/// `src/foo.rs` → `"foo"`, `lib.rs` → `"lib"`. Normalises Windows
/// backslashes. Delegates to the canonical `coupling::file_to_module`.
/// Trivial: single-delegation wrapper.
fn top_level_module(path: &str) -> String {
    crate::adapters::analyzers::coupling::file_to_module(path)
}
