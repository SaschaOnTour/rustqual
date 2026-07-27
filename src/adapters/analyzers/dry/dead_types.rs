//! DRY-006: types and constants nothing refers to.
//!
//! The counterpart of DRY-002 for everything that is not a function. Both use
//! the same shape — declarations on one side, a workspace-wide reference set on
//! the other — and the same exemptions, so an author does not have to learn two
//! rules: `#[allow(dead_code)]`, `// qual:api` and `// qual:test_helper` mean
//! here exactly what they mean there.
//!
//! What a declaration refers to is not the same as what keeps it alive: two
//! types naming each other mention both names while nothing reaches either. The
//! verdict therefore comes from `liveness` — a walk from the roots — and this
//! module supplies its seeds.

use std::collections::HashSet;

use super::liveness;
use super::type_references::collect_reference_graph;
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
    let mut declared = super::collect_declared_types(parsed, cfg_test_files);
    mark_annotated(&mut declared, api_lines, |d| d.is_api = true);
    mark_annotated(&mut declared, test_helper_lines, |d| {
        d.is_test_helper = true
    });
    let graph = collect_reference_graph(parsed, cfg_test_files);
    let alive = liveness::resolve(
        &graph,
        &owner_candidates(&declared),
        &excused_names(&declared),
    );
    let mut found = find_unused(&declared, &alive.production, &alive.tests);
    found.extend(find_test_only(&declared, &alive.production, &alive.tests));
    found
}

/// Every declaration that can own references. What the graph attributes to
/// anything else — a foreign type an extension trait is implemented for, a
/// blanket impl's type parameter — is rooted instead, since no verdict can ever
/// reach it.
///
/// A **type alias is not an owner**: `impl Alias { … }` attaches its items to
/// the aliased type, so whether the alias itself is used says nothing about
/// them, and answering the one question with the other reported what those
/// methods name as dead. The alias is still judged as a declaration; only its
/// role as an owner is dropped, which costs one thing — a type named solely by
/// an unused alias stays alive.
/// Operation: filter + collect, own calls hidden in the closures.
fn owner_candidates(declared: &[DeclaredType]) -> HashSet<String> {
    declared
        .iter()
        .filter(|d| d.kind != TypeItemKind::TypeAlias)
        .map(|d| d.name.clone())
        .collect()
}

/// Declarations whose own liveness is not in question, so what they name has a
/// real user: `#[allow(dead_code)] struct Kept { n: Named }` is a reason for
/// `Named` to exist, and reporting it would send the author to delete a field's
/// type. `// qual:test_helper` is in the list even though it does not excuse
/// `unused` — the marker says the tests are its users, and marking a fixture
/// must not then send the author to annotate every type the fixture names, one
/// by one. They contribute their references, not their own name: being excused
/// from a finding is not the same as being alive.
/// Operation: filter + collect, own calls hidden in the closures.
fn excused_names(declared: &[DeclaredType]) -> HashSet<String> {
    declared
        .iter()
        .filter(|d| excluded(d) || d.is_test_helper)
        .map(|d| d.name.clone())
        .collect()
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
                "only used from test code (a doc example counts); annotate with \
                 // qual:api if it is public API, // qual:test_helper if it serves \
                 the tests, or move it to the tests",
            )
        })
        .collect()
}

/// Exemptions shared by both findings. A leading underscore is the language's
/// own "deliberately unused" convention — rustc's `dead_code` lint honours it,
/// and a check that argued with the compiler here would just be noise.
/// Operation: boolean combination, no own calls.
fn excluded(d: &DeclaredType) -> bool {
    d.is_test || d.dead_code_exempt || d.is_api || d.name.starts_with('_')
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
