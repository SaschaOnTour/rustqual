//! Detection of `#[cfg(test)]`-reachable files across the parsed tree.
//!
//! This module identifies which source files are test-only — both those
//! declared directly with `#[cfg(test)] mod foo;` and their transitive
//! `mod` descendants. The dead-code and test-quality analyzers use the
//! resulting set to classify functions as test helpers rather than
//! production code.

use std::collections::HashSet;

/// Borrowed workspace slice shape. The test-file detector never needs
/// the source content (the middle `String` in the pipeline's parsed
/// tuple), just path + AST — so the internal helpers only take these.
/// Adapters (`collect_cfg_test_file_paths`) translate from their
/// richer tuple shape without cloning the ASTs.
pub(crate) use super::child_paths::{ChildPathResolver, ParsedRefs};
use super::crate_roots::crate_root_of;

/// Compute the set of source paths that are reachable only under
/// `#[cfg(test)]`. Combines direct hits with transitive propagation
/// through plain `mod name;` chains inside test-only files.
/// Also includes workspace-root `tests/**/*.rs` files, which Cargo
/// compiles exclusively as integration-test binaries.
pub(crate) fn collect_cfg_test_file_paths(
    parsed: &[(String, String, syn::File)],
) -> HashSet<String> {
    let refs: Vec<(&str, &syn::File)> = parsed.iter().map(|(p, _, f)| (p.as_str(), f)).collect();
    collect_cfg_test_file_paths_from_refs(&refs)
}

/// Borrowed variant for callers that don't have owned `syn::File`
/// tuples on hand (e.g. the architecture analyzer running over
/// `AnalysisContext`). Semantics identical to the owned form — the
/// detector never reads the source content String, only path + AST.
pub(crate) fn collect_cfg_test_file_paths_from_refs(parsed: &ParsedRefs<'_>) -> HashSet<String> {
    let resolver = ChildPathResolver::from_parsed(parsed);
    let mut set = direct_cfg_test_files(parsed, &resolver);
    set.extend(inner_cfg_test_files(parsed));
    set.extend(integration_test_files(parsed));
    propagate_cfg_test_through_plain_mods(parsed, &resolver, &mut set);
    set
}

/// Files with a top-level `#![cfg(test)]` inner attribute — the Rust
/// convention for "this whole file is test-only", commonly used on
/// companion `*_tests.rs` files linked via `#[path]` redirects.
/// Operation: iterates parsed files checking file-level attrs.
fn inner_cfg_test_files(parsed: &ParsedRefs<'_>) -> HashSet<String> {
    parsed
        .iter()
        .filter(|(_, file)| super::cfg_test::has_cfg_test(&file.attrs))
        .map(|(path, _)| path.to_string())
        .collect()
}

/// Yields the owner directory of **every** `tests/` directory component in
/// `path` (`""` when `tests` is the leading component). A path may contain
/// more than one — e.g. a package nested under a directory literally named
/// `tests` (`fixtures/tests/retry/tests/it.rs`) has owners `fixtures` and
/// `fixtures/tests/retry`; the caller checks each against the package roots,
/// so the package's own `tests/` is matched at the right root. Boundary-
/// aware: `mytests/x.rs` does not match.
/// Operation: segment scan + slicing, own logic hidden in the closure.
fn tests_dir_owners(path: &str) -> impl Iterator<Item = &str> + '_ {
    path.char_indices().filter_map(move |(i, _)| {
        let at_segment_start = i == 0 || path.as_bytes()[i - 1] == b'/';
        (at_segment_start && path[i..].starts_with("tests/")).then(|| {
            if i == 0 {
                ""
            } else {
                &path[..i - 1]
            }
        })
    })
}

/// Directory prefixes that are genuine Cargo package roots in the parsed
/// tree: every directory that holds a crate-root file. `""` denotes the
/// analysis-root crate. Which files count as roots is decided by the shared
/// [`crate_root_of`] — so a directory merely *containing* a `src/` subtree
/// (without a crate root) does not qualify.
/// Operation: filter_map over parsed paths, own call hidden in the closure.
fn package_roots(parsed: &ParsedRefs<'_>) -> HashSet<String> {
    parsed
        .iter()
        .filter_map(|(path, _)| crate_root_of(path).map(|(owner, _)| owner.to_string()))
        .collect()
}

/// True for source paths Cargo compiles as integration-test binaries:
/// a file inside a `tests/` directory at a **package root** — the
/// directory owning that `tests/` must itself be a package root (i.e. be
/// in `package_roots`, derived from crate-root files). This matches
/// `tests/foo.rs` (analysis-root crate) and `crates/<name>/tests/foo.rs`,
/// but NOT a `tests/` directory without a sibling crate root
/// (`fixtures/tests/...`, `tools/shared/tests/...`, a coincidental
/// `src/`+`tests/` pair) nor one nested under a package's `src/`
/// (`src/foo/tests/bar.rs`, a unit-test submodule reached via
/// `#[cfg(test)] mod`).
///
/// This is the single source of truth for the integration-test path
/// rule; it feeds only the cfg-test file set, which every dimension then
/// consults — so the rule cannot drift. Callers pass forward-slash
/// normalised paths.
/// Operation: owner lookup + set membership, no own calls.
pub(crate) fn is_integration_test_path(path: &str, package_roots: &HashSet<String>) -> bool {
    tests_dir_owners(path).any(|owner| package_roots.contains(owner))
}

/// Files Cargo automatically treats as integration tests — everything
/// inside a package-root `tests/` directory. Each is its own test
/// binary; no production code lives there.
/// Operation: derive package roots, then filter, no logic.
fn integration_test_files(parsed: &ParsedRefs<'_>) -> HashSet<String> {
    let roots = package_roots(parsed);
    parsed
        .iter()
        .map(|(path, _)| *path)
        .filter(|p| is_integration_test_path(p, &roots))
        .map(String::from)
        .collect()
}

/// One out-of-line `mod name;` a file declares, with what locating and
/// classifying it needs: the inline `mod {}` chain enclosing it, and whether
/// any block in that chain carries `#[cfg(test)]`.
struct ExternalMod<'a> {
    inline_stack: Vec<String>,
    under_cfg_test: bool,
    item: &'a syn::ItemMod,
}

/// Every out-of-line `mod name;` in a file, descending through inline
/// `mod {}` blocks. A declaration nested in one is a real declaration: it just
/// needs the enclosing chain to locate its file, since a module's children live
/// in *its* directory, not the file's.
/// Integration: per-item delegation.
// qual:recursive
fn external_mods<'a>(
    items: &'a [syn::Item],
    inline_stack: &[String],
    under_cfg_test: bool,
) -> Vec<ExternalMod<'a>> {
    items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Mod(m) => Some(m),
            _ => None,
        })
        .flat_map(|m| mod_declarations(m, inline_stack, under_cfg_test))
        .collect()
}

/// One `mod` item: an out-of-line declaration is itself the result; an inline
/// block contributes what its body declares, one level deeper — and passes its
/// own `#[cfg(test)]` down, because the attribute covers everything inside.
/// Operation: selects between the two shapes, own calls hidden in the closures.
// qual:recursive
fn mod_declarations<'a>(
    m: &'a syn::ItemMod,
    inline_stack: &[String],
    under_cfg_test: bool,
) -> Vec<ExternalMod<'a>> {
    let declaration = || {
        vec![ExternalMod {
            inline_stack: inline_stack.to_vec(),
            under_cfg_test,
            item: m,
        }]
    };
    let descend = |inner: &'a [syn::Item]| {
        external_mods(
            inner,
            &child_stack(inline_stack, m),
            under_cfg_test || super::cfg_test::has_cfg_test(&m.attrs),
        )
    };
    m.content
        .as_ref()
        .map_or_else(declaration, |(_, inner)| descend(inner))
}

/// Operation: clone + push, no own calls.
fn child_stack(inline_stack: &[String], m: &syn::ItemMod) -> Vec<String> {
    let mut nested = inline_stack.to_vec();
    nested.push(m.ident.to_string());
    nested
}

/// Files referenced by an explicit `#[cfg(test)] mod foo;` in a parent file —
/// including one nested in an inline `mod {}` block, or covered by a
/// `#[cfg(test)]` on such a block.
fn direct_cfg_test_files(
    parsed: &ParsedRefs<'_>,
    resolver: &ChildPathResolver<'_>,
) -> HashSet<String> {
    let is_cfg_test =
        |e: &ExternalMod<'_>| e.under_cfg_test || super::cfg_test::has_cfg_test(&e.item.attrs);
    parsed
        .iter()
        .flat_map(|(path, file)| {
            external_mods(&file.items, &[], false)
                .into_iter()
                .filter(&is_cfg_test)
                .filter_map(|e| resolver.resolve(path, &e.inline_stack, e.item))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Propagate cfg-test status through plain `mod foo;` chains until fix-point.
/// A sub-module declared inside an already-cfg-test file becomes cfg-test too.
fn propagate_cfg_test_through_plain_mods(
    parsed: &ParsedRefs<'_>,
    resolver: &ChildPathResolver<'_>,
    set: &mut HashSet<String>,
) {
    let path_to_file: std::collections::HashMap<&str, &syn::File> =
        parsed.iter().map(|(p, f)| (*p, *f)).collect();
    loop {
        let new_children: Vec<String> = set
            .iter()
            .filter_map(|parent_path| {
                path_to_file
                    .get(parent_path.as_str())
                    .map(|f| (parent_path, *f))
            })
            .flat_map(|(parent_path, file)| {
                external_mods(&file.items, &[], false)
                    .into_iter()
                    .filter_map(|e| resolver.resolve(parent_path, &e.inline_stack, e.item))
                    .collect::<Vec<_>>()
            })
            .filter(|child| !set.contains(child))
            .collect();
        if new_children.is_empty() {
            break;
        }
        set.extend(new_children);
    }
}
