use crate::adapters::shared::file_to_module::file_to_module;
use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralMetadata, StructuralWarning, StructuralWarningKind};

/// Detect orphaned impls: inherent impl in a different **top-level
/// module** than the type definition (sibling files under the same
/// module — e.g. `analyzer/mod.rs` and `analyzer/types.rs` — are
/// intentionally allowed). Impls in entirely different modules are
/// flagged as "defined elsewhere".
///
/// Types are known by bare name, so several definitions can answer for one
/// impl. Which one the impl means is not knowable here, so the impl is
/// orphaned only when *none* of them shares its module — a verdict that holds
/// whichever it means, and does not depend on the order files were read. The
/// finding names every candidate, sorted: picking one would be a guess. The
/// path of an `impl crate::b::Thing` header is not used to narrow them: which
/// file holds module `b`'s `Thing` is a resolution question (re-exports,
/// `#[path]`), and reading it as "the file `b.rs`" reported correct impls as
/// orphaned. That waits for module-path identity (#60).
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
            let defs = meta
                .type_defs
                .get(type_name)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let Some((defs, at_home)) = placement(meta, defs, impl_file) else {
                return;
            };
            if !at_home && !defs.is_empty() {
                let mut candidates = defs.to_vec();
                candidates.sort();
                warnings.push(StructuralWarning {
                    file: impl_file.clone(),
                    line: *impl_line,
                    name: type_name.clone(),
                    kind: StructuralWarningKind::OrphanedImpl {
                        defining_file: candidates.join(" or "),
                    },
                    dimension: Dimension::Coupling,
                    suppressed: false,
                });
            }
        });
}

/// The candidates an impl can mean and whether one shares its module.
///
/// Where the module tree walk knows both sides, modules are *logical* — under
/// `#[path = "types.rs"] mod a`, `types.rs` and a child it declares are both
/// module `a`, and comparing file names reported such an impl as orphaned. A
/// file mounted twice has two places, and a match at any of them counts.
/// Only candidates of the impl's own crates count, since an inherent impl can
/// only be for a type of its own crate. `None` means no verdict: no candidate
/// placed in the impl's crate (a type the analysis did not see, or a file the
/// walk could not follow), or the impl's own file unplaced. Only where the
/// walk knows nothing at all does the file-name comparison stay.
/// Operation: lookups and filters, own calls in the closures.
fn placement(
    meta: &StructuralMetadata,
    defs: &[String],
    impl_file: &str,
) -> Option<(Vec<String>, bool)> {
    let places = |f: &str| meta.file_modules.get(f).map(Vec::as_slice).unwrap_or(&[]);
    let by_file_name = || {
        let impl_module = file_to_module(impl_file);
        let at_home = defs.iter().any(|d| file_to_module(d) == impl_module);
        (defs.to_vec(), at_home)
    };
    // No walk at all (loose files, no crate root): file names are all there
    // is. A walk that knows the crate but not this file — a `cfg_attr(path =
    // …)` it cannot follow — means the placement is unknown: no verdict.
    if meta.file_modules.is_empty() {
        return Some(by_file_name());
    }
    let impl_places = places(impl_file);
    if impl_places.is_empty() {
        return None;
    }
    let shares_crate = |d: &&String| {
        places(d)
            .iter()
            .any(|(c, _)| impl_places.iter().any(|(ic, _)| ic == c))
    };
    let same_crate: Vec<String> = defs.iter().filter(shares_crate).cloned().collect();
    if same_crate.is_empty() {
        return None;
    }
    let at_home = same_crate
        .iter()
        .any(|d| places(d).iter().any(|p| impl_places.contains(p)));
    Some((same_crate, at_home))
}
