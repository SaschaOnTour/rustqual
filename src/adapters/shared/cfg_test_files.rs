//! Detection of `#[cfg(test)]`-reachable files across the parsed tree.
//!
//! This module identifies which source files are test-only — both those
//! declared directly with `#[cfg(test)] mod foo;` and their transitive
//! `mod` descendants. The dead-code and test-quality analyzers use the
//! resulting set to classify functions as test helpers rather than
//! production code.

use std::collections::HashSet;
use std::path::Path;

/// Borrowed workspace slice shape. The test-file detector never needs
/// the source content (the middle `String` in the pipeline's parsed
/// tuple), just path + AST — so the internal helpers only take these.
/// Adapters (`collect_cfg_test_file_paths`) translate from their
/// richer tuple shape without cloning the ASTs.
type ParsedRefs<'a> = [(&'a str, &'a syn::File)];

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

/// Files Cargo automatically treats as integration tests — everything
/// under the workspace-root `tests/` directory. Each is its own test
/// binary; no production code lives there. Companion test subtrees
/// under `src/**/tests/` are already reached via the `#[cfg(test)] mod`
/// detection above.
/// Operation: path-prefix filter, no own calls.
fn integration_test_files(parsed: &ParsedRefs<'_>) -> HashSet<String> {
    parsed
        .iter()
        .map(|(path, _)| *path)
        .filter(|p| p.starts_with("tests/"))
        .map(String::from)
        .collect()
}

/// Resolves `mod name;` declarations to child file paths by probing the
/// candidate `{parent_dir}/{name}.rs` and `{parent_dir}/{name}/mod.rs`
/// locations against the set of known file paths.
struct ChildPathResolver<'a> {
    known_paths: HashSet<&'a str>,
}

impl<'a> ChildPathResolver<'a> {
    fn from_parsed(parsed: &'a ParsedRefs<'a>) -> Self {
        Self {
            known_paths: parsed.iter().map(|(p, _)| *p).collect(),
        }
    }

    fn resolve(&self, parent_path: &str, mod_item: &syn::ItemMod) -> Option<String> {
        if let Some(explicit) = path_attribute(&mod_item.attrs) {
            return self.resolve_explicit_path(parent_path, &explicit);
        }
        self.resolve_by_convention(parent_path, &mod_item.ident.to_string())
    }

    /// `#[path = "custom.rs"]` is resolved relative to the directory
    /// containing the parent file, matching rustc's own semantics.
    /// Operation: path arithmetic + existence check, no own calls.
    fn resolve_explicit_path(&self, parent_path: &str, relative: &str) -> Option<String> {
        let parent_dir = Path::new(parent_path)
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf();
        let candidate = parent_dir
            .join(relative)
            .to_string_lossy()
            .replace('\\', "/");
        self.known_paths
            .contains(candidate.as_str())
            .then_some(candidate)
    }

    /// Naming-convention resolution: try `{dir}/{name}.rs` then
    /// `{dir}/{name}/mod.rs` under the parent file's module directory.
    /// Operation: path arithmetic + existence checks, no own calls.
    fn resolve_by_convention(&self, parent_path: &str, mod_name: &str) -> Option<String> {
        let parent = Path::new(parent_path);
        let child_dir = if parent
            .file_stem()
            .is_some_and(|s| s == "mod" || s == "lib" || s == "main")
        {
            parent.parent().unwrap_or(Path::new("")).to_path_buf()
        } else {
            parent.with_extension("")
        };
        let candidate_file = child_dir
            .join(format!("{mod_name}.rs"))
            .to_string_lossy()
            .into_owned();
        let candidate_dir = child_dir
            .join(mod_name)
            .join("mod.rs")
            .to_string_lossy()
            .into_owned();
        if self.known_paths.contains(candidate_file.as_str()) {
            Some(candidate_file)
        } else if self.known_paths.contains(candidate_dir.as_str()) {
            Some(candidate_dir)
        } else {
            None
        }
    }
}

/// Extract the string value of a `#[path = "..."]` attribute if present.
/// Operation: attribute lookup + literal parsing, no own calls.
fn path_attribute(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find_map(|attr| {
        if !attr.path().is_ident("path") {
            return None;
        }
        match &attr.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(expr_lit) => match &expr_lit.lit {
                    syn::Lit::Str(s) => Some(s.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    })
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
