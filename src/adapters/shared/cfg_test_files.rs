//! Detection of `#[cfg(test)]`-reachable files across the parsed tree.
//!
//! This module identifies which source files are test-only — both those
//! declared directly with `#[cfg(test)] mod foo;` and their transitive
//! `mod` descendants. The dead-code and test-quality analyzers use the
//! resulting set to classify functions as test helpers rather than
//! production code.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::Path;

/// Borrowed workspace slice shape. The test-file detector never needs
/// the source content (the middle `String` in the pipeline's parsed
/// tuple), just path + AST — so the internal helpers only take these.
/// Adapters (`collect_cfg_test_file_paths`) translate from their
/// richer tuple shape without cloning the ASTs.
pub(crate) use super::child_paths::{ChildPathResolver, ParsedRefs};

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

/// If `path` is a crate-root file, return the owning package-root
/// directory (`""` for the analysis-root crate). Crate roots are Cargo's
/// defining markers of a package: the default `src/lib.rs` / `src/main.rs`
/// and autobinary `src/bin/<name>.rs`. This identifies real package roots
/// from the parsed `.rs` set without reading any manifest. Custom
/// `[lib] path = …` / `[[bin]] path = …` layouts are not detectable from
/// paths alone (they would require parsing `Cargo.toml`).
/// Integration: combines the default-root and autobinary lookups.
fn crate_root_owner(path: &str) -> Option<&str> {
    ["src/lib.rs", "src/main.rs"]
        .into_iter()
        .find_map(|tail| owner_with_tail(path, tail))
        .or_else(|| bin_crate_root_owner(path))
}

/// Owner directory of a path ending in `<owner>/{tail}` (`""` when the
/// path equals `tail`), or `None`. Boundary-aware via the trailing `/`.
/// Operation: suffix matching, no own calls.
fn owner_with_tail<'a>(path: &'a str, tail: &str) -> Option<&'a str> {
    (path == tail)
        .then_some("")
        .or_else(|| path.strip_suffix(tail).and_then(|p| p.strip_suffix('/')))
}

/// Owner directory of an autobinary crate root `<owner>/src/bin/<name>.rs`
/// (`""` for a top-level `src/bin/<name>.rs`), or `None`. Only a file
/// directly inside `src/bin/` counts — deeper modules do not.
/// Operation: split + filter, no own calls.
fn bin_crate_root_owner(path: &str) -> Option<&str> {
    path.strip_prefix("src/bin/")
        .map(|name| ("", name))
        .or_else(|| path.split_once("/src/bin/"))
        .filter(|(_, name)| name.ends_with(".rs") && !name.contains('/'))
        .map(|(owner, _)| owner)
}

/// Directory prefixes that are genuine Cargo package roots in the parsed
/// tree: every directory that holds a crate-root file (`src/lib.rs` or
/// `src/main.rs`). `""` denotes the analysis-root crate. Derived from the
/// actual `.rs` set — no filesystem or manifest access — so a directory
/// merely *containing* a `src/` subtree (without a crate root) does not
/// qualify.
/// Operation: filter_map over parsed paths, no own calls.
fn package_roots(parsed: &ParsedRefs<'_>) -> HashSet<String> {
    parsed
        .iter()
        .filter_map(|(path, _)| crate_root_owner(path).map(String::from))
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

/// Files referenced by an explicit `#[cfg(test)] mod foo;` in a parent file.
fn direct_cfg_test_files(
    parsed: &ParsedRefs<'_>,
    resolver: &ChildPathResolver<'_>,
) -> HashSet<String> {
    let is_ext_cfg_test =
        |m: &syn::ItemMod| m.content.is_none() && super::cfg_test::has_cfg_test(&m.attrs);
    parsed
        .iter()
        .flat_map(|(path, file)| {
            file.items
                .iter()
                .filter_map(move |item| match item {
                    syn::Item::Mod(m) if is_ext_cfg_test(m) => Some((*path, m)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .filter_map(|(parent, m)| resolver.resolve(parent, m))
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
    let is_any_ext_mod = |m: &syn::ItemMod| m.content.is_none();
    loop {
        let new_children: Vec<String> = set
            .iter()
            .filter_map(|parent_path| {
                path_to_file
                    .get(parent_path.as_str())
                    .map(|f| (parent_path, *f))
            })
            .flat_map(|(parent_path, file)| {
                file.items
                    .iter()
                    .filter_map(|item| match item {
                        syn::Item::Mod(m) if is_any_ext_mod(m) => resolver.resolve(parent_path, m),
                        _ => None,
                    })
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
