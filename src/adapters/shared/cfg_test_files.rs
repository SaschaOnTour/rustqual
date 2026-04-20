//! Detection of `#[cfg(test)]`-reachable files across the parsed tree.
//!
//! This module identifies which source files are test-only — both those
//! declared directly with `#[cfg(test)] mod foo;` and their transitive
//! `mod` descendants. The dead-code and test-quality analyzers use the
//! resulting set to classify functions as test helpers rather than
//! production code.

use std::collections::HashSet;
use std::path::Path;

/// Compute the set of source paths that are reachable only under
/// `#[cfg(test)]`. Combines direct hits with transitive propagation
/// through plain `mod name;` chains inside test-only files.
/// Also includes workspace-root `tests/**/*.rs` files, which Cargo
/// compiles exclusively as integration-test binaries.
pub(crate) fn collect_cfg_test_file_paths(
    parsed: &[(String, String, syn::File)],
) -> HashSet<String> {
    let resolver = ChildPathResolver::from_parsed(parsed);
    let mut set = direct_cfg_test_files(parsed, &resolver);
    set.extend(integration_test_files(parsed));
    propagate_cfg_test_through_plain_mods(parsed, &resolver, &mut set);
    set
}

/// Files Cargo automatically treats as integration tests — everything
/// under the workspace-root `tests/` directory. Each is its own test
/// binary; no production code lives there. Companion test subtrees
/// under `src/**/tests/` are already reached via the `#[cfg(test)] mod`
/// detection above.
/// Operation: path-prefix filter, no own calls.
fn integration_test_files(parsed: &[(String, String, syn::File)]) -> HashSet<String> {
    parsed
        .iter()
        .map(|(path, _, _)| path.as_str())
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
    fn from_parsed(parsed: &'a [(String, String, syn::File)]) -> Self {
        Self {
            known_paths: parsed.iter().map(|(p, _, _)| p.as_str()).collect(),
        }
    }

    fn resolve(&self, parent_path: &str, mod_name: &str) -> Option<String> {
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

/// Files referenced by an explicit `#[cfg(test)] mod foo;` in a parent file.
fn direct_cfg_test_files(
    parsed: &[(String, String, syn::File)],
    resolver: &ChildPathResolver<'_>,
) -> HashSet<String> {
    let is_ext_cfg_test =
        |m: &syn::ItemMod| m.content.is_none() && super::cfg_test::has_cfg_test(&m.attrs);
    parsed
        .iter()
        .flat_map(|(path, _, file)| {
            file.items
                .iter()
                .filter_map(|item| match item {
                    syn::Item::Mod(m) if is_ext_cfg_test(m) => {
                        Some((path.as_str(), m.ident.to_string()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .filter_map(|(parent, name)| resolver.resolve(parent, &name))
        .collect()
}

/// Propagate cfg-test status through plain `mod foo;` chains until fix-point.
/// A sub-module declared inside an already-cfg-test file becomes cfg-test too.
fn propagate_cfg_test_through_plain_mods(
    parsed: &[(String, String, syn::File)],
    resolver: &ChildPathResolver<'_>,
    set: &mut HashSet<String>,
) {
    let path_to_file: std::collections::HashMap<&str, &syn::File> =
        parsed.iter().map(|(p, _, f)| (p.as_str(), f)).collect();
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
                        syn::Item::Mod(m) if is_any_ext_mod(m) => {
                            resolver.resolve(parent_path, &m.ident.to_string())
                        }
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
