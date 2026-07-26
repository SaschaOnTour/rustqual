//! DRY-006: types and constants nothing refers to.
//!
//! The counterpart of DRY-002 for everything that is not a function. Both use
//! the same shape — declarations on one side, a workspace-wide reference set on
//! the other — and the same exemptions, so an author does not have to learn two
//! rules: `#[allow(dead_code)]`, `// qual:api` and `// qual:test_helper` mean
//! here exactly what they mean there.

use std::collections::HashSet;

use super::type_references::collect_type_references;
use crate::adapters::shared::declared_type::{DeclaredType, TypeItemKind};
use crate::adapters::shared::marked_declaration::{mark_annotated, MarkerLines};

/// Why a declaration is dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadTypeKind {
    /// Nothing anywhere refers to it.
    Unused,
    /// Only test code refers to it.
    TestOnly,
}

/// A warning about a declaration nothing (or only test code) refers to.
#[derive(Debug, Clone)]
pub struct DeadTypeWarning {
    pub name: String,
    /// What was declared — `struct`, `const`, … — so the message can say so.
    pub item: TypeItemKind,
    pub file: String,
    pub line: usize,
    pub kind: DeadTypeKind,
    pub suggestion: String,
}

/// Detect dead types and constants across parsed files.
/// Integration: collection, marking, then the two findings.
/// Note: the `detect_dead_types` config flag is checked by the pipeline caller.
pub fn detect_dead_types(
    parsed: &[(String, String, syn::File)],
    api_lines: &MarkerLines,
    test_helper_lines: &MarkerLines,
    cfg_test_files: &HashSet<String>,
) -> Vec<DeadTypeWarning> {
    let mut declared = collect_declared(parsed, cfg_test_files);
    mark_annotated(&mut declared, api_lines, |d| d.is_api = true);
    mark_annotated(&mut declared, test_helper_lines, |d| {
        d.is_test_helper = true
    });
    let (production, tests) = collect_type_references(parsed, cfg_test_files);
    let mut found = find_unused(&declared, &production, &tests);
    found.extend(find_test_only(&declared, &production, &tests));
    found
}

/// Collect declarations and mark those from test-only files.
/// Operation: collection + flag pass, own calls hidden in the closures.
fn collect_declared(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &HashSet<String>,
) -> Vec<DeclaredType> {
    let mut declared = super::collect_declared_types(parsed);
    declared
        .iter_mut()
        .filter(|d| cfg_test_files.contains(&d.file))
        .for_each(|d| d.is_test = true);
    declared
}

/// Declarations nothing refers to, from production or test code.
/// `// qual:test_helper` does NOT exclude here — a helper nothing refers to is
/// dead whatever the marker claims, mirroring DRY-002.
/// Operation: set logic + filtering, own calls hidden in the closures.
fn find_unused(
    declared: &[DeclaredType],
    production: &HashSet<String>,
    tests: &HashSet<String>,
) -> Vec<DeadTypeWarning> {
    declared
        .iter()
        .filter(|d| !excluded(d))
        .filter(|d| !production.contains(&d.name) && !tests.contains(&d.name))
        .map(|d| warning(d, DeadTypeKind::Unused, "never used; consider removing"))
        .collect()
}

/// Declarations only test code refers to.
/// Operation: set logic + filtering, own calls hidden in the closures.
fn find_test_only(
    declared: &[DeclaredType],
    production: &HashSet<String>,
    tests: &HashSet<String>,
) -> Vec<DeadTypeWarning> {
    declared
        .iter()
        .filter(|d| !d.is_test_helper && !excluded(d))
        .filter(|d| tests.contains(&d.name) && !production.contains(&d.name))
        .map(|d| {
            warning(
                d,
                DeadTypeKind::TestOnly,
                "only used from test code; move it to the tests, or annotate with \
                 // qual:api (public API) or // qual:test_helper (test-only helper)",
            )
        })
        .collect()
}

/// Exemptions shared by both findings. A leading underscore is the language's
/// own "deliberately unused" convention — rustc's `dead_code` lint honours it,
/// and a check that argued with the compiler here would just be noise.
/// Operation: boolean combination, no own calls.
fn excluded(d: &DeclaredType) -> bool {
    d.is_test || d.has_allow_dead_code || d.is_api || d.name.starts_with('_')
}

/// Build one warning, naming the kind of declaration it is about.
/// Operation: struct construction, no own calls.
fn warning(d: &DeclaredType, kind: DeadTypeKind, advice: &str) -> DeadTypeWarning {
    DeadTypeWarning {
        name: d.name.clone(),
        item: d.kind,
        file: d.file.clone(),
        line: d.line,
        kind,
        suggestion: format!("{} `{}` is {advice}", d.kind.label(), d.name),
    }
}
