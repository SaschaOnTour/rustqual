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
    // Sorted: `trait_defs` is a `HashMap`, and iterating it directly put the
    // same findings out in a different order from run to run.
    let mut traits: Vec<_> = meta.trait_defs.iter().collect();
    traits.sort_by(|a, b| a.0.cmp(b.0));
    traits.into_iter().for_each(|(trait_name, defs)| {
        // Impls are counted by bare trait name. With two traits of one name,
        // which impl belongs to which is not knowable — and the last one read
        // took the finding, on a trait with no implementor. Undecidable means
        // no finding.
        let [info] = defs.as_slice() else {
            return;
        };
        // Skip pub traits (may have external impls) and ones behind a cfg
        if info.is_pub || info.gated {
            return;
        }
        // Skip marker traits (no methods)
        if info.method_count == 0 {
            return;
        }
        // Unwrapped in one place, no `[trait_name][0]` index that could panic.
        let Some(impls) = meta.trait_impls.get(trait_name) else {
            return;
        };
        let [(impl_type, impl_file, certain_path)] = impls.as_slice() else {
            return;
        };
        if !certainly_this_trait(meta, trait_name, &info.file, impl_file, *certain_path) {
            return;
        }
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

/// Whether the one impl counted under `trait_name` certainly implements the
/// private trait declared in `trait_file` — the only case SIT may report.
///
/// Certain is narrow on purpose: trait and impl in one file, the trait named
/// bare (no prelude name, no cfg — `certain_path`), and nothing in the file
/// binding that name by `use` or glob. Without an import a bare name sees
/// only what its own module declares, so it can only be this trait. Every
/// wider rule tried — anything under the trait's module, `self::`/`super::`
/// paths, "foreign" judged by path roots — had a route by which a foreign
/// trait (the prelude, a facade's re-export, a glob) reached the name and a
/// private trait nobody implements was reported as single-impl.
/// Operation: equality and membership checks, no own calls.
fn certainly_this_trait(
    meta: &StructuralMetadata,
    trait_name: &str,
    trait_file: &str,
    impl_file: &str,
    certain_path: bool,
) -> bool {
    let imported = meta
        .outside_imports
        .contains(&(impl_file.to_string(), trait_name.to_string()));
    certain_path && impl_file == trait_file && !imported && !meta.glob_files.contains(impl_file)
}
