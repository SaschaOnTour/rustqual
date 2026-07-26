//! Can an item be named from *outside* its crate?
//!
//! `// qual:api` exists to excuse an entry point whose callers live outside the
//! analysed code. That excuse only makes sense for items an outside consumer
//! can actually reach: `pub`, behind an unbroken chain of `pub mod`, in a
//! library crate. An item behind a private `mod` is unreachable no matter how
//! many `pub` keywords it carries — a `qual:api` there is a category error.
//!
//! The derivation is pure `.rs`-set analysis (no `Cargo.toml`, matching the
//! rest of rustqual): file paths give module paths, `mod` declarations give
//! the visibility chain, and `pub use` re-exports rescue items from private
//! modules. Every uncertainty resolves to **reachable**, so an unusual layout
//! can never manufacture a false "this marker does not apply".

use std::collections::{HashMap, HashSet};

/// Which items a consumer outside the crate can name.
pub(crate) struct ExternalReach {
    /// Files whose module path is reachable through `pub mod` links.
    reachable_files: HashSet<String>,
    /// `(file, fn-name)` pairs that are `pub` through their inline-mod chain.
    pub_items: HashSet<(String, String)>,
    /// Item names re-exported via `pub use path::Name`.
    reexported_names: HashSet<String>,
    /// Files whose items are all re-exported via `pub use path::*`.
    glob_reexported_files: HashSet<String>,
    /// Files the layout derivation did not recognise — treated as reachable.
    unknown_files: HashSet<String>,
}

impl ExternalReach {
    /// True when `name` in `file` can be named from outside the crate.
    /// Integration: combines the layout, visibility and re-export facts.
    pub(crate) fn is_externally_reachable(&self, file: &str, name: &str) -> bool {
        if self.unknown_files.contains(file) || self.reexported_names.contains(name) {
            return true;
        }
        let is_pub = self
            .pub_items
            .contains(&(file.to_string(), name.to_string()));
        if self.glob_reexported_files.contains(file) {
            return is_pub;
        }
        is_pub && self.reachable_files.contains(file)
    }
}

/// Build the reachability facts for the whole parsed set.
/// Integration: per-file facts, then a walk down the `pub mod` links.
pub(crate) fn compute_external_reach(parsed: &[(String, String, syn::File)]) -> ExternalReach {
    let modules = module_index(parsed);
    let mut facts = FileFacts::default();
    parsed
        .iter()
        .for_each(|(file, _, syntax)| collect_file_facts(file, syntax, &mut facts));
    let reachable_files = walk_reachable(parsed, &modules, &facts);
    let glob_reexported_files = resolve_globs(&facts.glob_uses, &modules);
    ExternalReach {
        reachable_files,
        pub_items: facts.pub_items,
        reexported_names: facts.reexported_names,
        glob_reexported_files,
        unknown_files: parsed
            .iter()
            .map(|(f, _, _)| f.clone())
            .filter(|f| module_path_of(f).is_none())
            .collect(),
    }
}

/// Per-file syntax facts gathered in one pass.
#[derive(Default)]
struct FileFacts {
    /// `(file, fn-name)` for functions `pub` through their inline-mod chain.
    pub_items: HashSet<(String, String)>,
    /// `(file, child-module-name)` for `pub mod child;` declarations.
    pub_mod_links: HashSet<(String, String)>,
    /// Names re-exported by a `pub use …::Name;`.
    reexported_names: HashSet<String>,
    /// Module paths glob-re-exported by a `pub use path::*;`.
    glob_uses: HashSet<String>,
}

/// Walk one file's items, recording visibility chains and re-exports.
/// Operation: item scan delegating to the recursive module walker.
fn collect_file_facts(file: &str, syntax: &syn::File, facts: &mut FileFacts) {
    let base = module_path_of(file).unwrap_or_default();
    walk_items(file, &syntax.items, true, &base, facts);
}

/// Recurse through items, carrying whether the enclosing chain is fully `pub`.
/// `mod_path` is the module path of the current (possibly inline) scope, used
/// to resolve `pub use self::…` globs. Operation: item dispatch + recursion.
// qual:recursive
fn walk_items(
    file: &str,
    items: &[syn::Item],
    chain_is_pub: bool,
    mod_path: &[String],
    facts: &mut FileFacts,
) {
    for item in items {
        match item {
            syn::Item::Fn(f) if chain_is_pub && is_pub(&f.vis) => {
                facts
                    .pub_items
                    .insert((file.to_string(), f.sig.ident.to_string()));
            }
            syn::Item::Mod(m) => {
                let child_pub = chain_is_pub && is_pub(&m.vis);
                match &m.content {
                    Some((_, inner)) => {
                        let mut nested = mod_path.to_vec();
                        nested.push(m.ident.to_string());
                        walk_items(file, inner, child_pub, &nested, facts);
                    }
                    // Out-of-line `mod x;` — the link's visibility decides
                    // whether the file implementing it stays reachable.
                    None if child_pub => {
                        facts
                            .pub_mod_links
                            .insert((file.to_string(), m.ident.to_string()));
                    }
                    None => {}
                }
            }
            syn::Item::Use(u) if chain_is_pub && is_pub(&u.vis) => {
                collect_use(&u.tree, mod_path, &mut Vec::new(), facts);
            }
            _ => {}
        }
    }
}

/// Record what a `pub use` tree exposes: concrete names, and module paths
/// whose whole surface is re-exported by a glob.
/// Operation: use-tree recursion accumulating the path prefix.
// qual:recursive
fn collect_use(
    tree: &syn::UseTree,
    mod_path: &[String],
    prefix: &mut Vec<String>,
    facts: &mut FileFacts,
) {
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            collect_use(&p.tree, mod_path, prefix, facts);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            facts.reexported_names.insert(n.ident.to_string());
        }
        syn::UseTree::Rename(r) => {
            // The *source* name is what keeps the original item public.
            facts.reexported_names.insert(r.ident.to_string());
        }
        syn::UseTree::Glob(_) => {
            facts.glob_uses.insert(resolve_use_prefix(mod_path, prefix));
        }
        syn::UseTree::Group(g) => g
            .items
            .iter()
            .for_each(|t| collect_use(t, mod_path, &mut prefix.clone(), facts)),
    }
}

/// Turn a `use` prefix into an absolute module path, resolving the
/// `crate::`/`self::` heads against the current scope.
/// Operation: prefix normalisation, no own calls.
fn resolve_use_prefix(mod_path: &[String], prefix: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut rest = prefix;
    match rest.first().map(String::as_str) {
        Some("crate") => rest = &rest[1..],
        Some("self") => {
            out.extend(mod_path.iter().cloned());
            rest = &rest[1..];
        }
        _ => out.extend(mod_path.iter().cloned()),
    }
    out.extend(rest.iter().cloned());
    out.join("::")
}

/// Map module path → file, for every file whose layout we understand.
/// Operation: path derivation over the parsed set.
fn module_index(parsed: &[(String, String, syn::File)]) -> HashMap<String, String> {
    parsed
        .iter()
        .filter_map(|(file, _, _)| module_path_of(file).map(|p| (p.join("::"), file.clone())))
        .collect()
}

/// Files reachable from a crate root through `pub mod` links.
/// Operation: fixpoint expansion over the module links.
fn walk_reachable(
    parsed: &[(String, String, syn::File)],
    modules: &HashMap<String, String>,
    facts: &FileFacts,
) -> HashSet<String> {
    // Library roots only: a binary has no outside consumers, so nothing in it
    // is externally reachable.
    let mut reachable: HashSet<String> = parsed
        .iter()
        .map(|(f, _, _)| f.clone())
        .filter(|f| is_lib_root(f))
        .collect();
    let mut changed = true;
    while changed {
        changed = false;
        for (parent, child) in &facts.pub_mod_links {
            if !reachable.contains(parent) {
                continue;
            }
            let Some(parent_path) = module_path_of(parent) else {
                continue;
            };
            let mut child_path = parent_path;
            child_path.push(child.clone());
            if let Some(child_file) = modules.get(&child_path.join("::")) {
                changed |= reachable.insert(child_file.clone());
            }
        }
    }
    reachable
}

/// Files covered by a `pub use <module>::*` glob.
/// Operation: glob path → file lookup.
fn resolve_globs(globs: &HashSet<String>, modules: &HashMap<String, String>) -> HashSet<String> {
    globs
        .iter()
        .filter_map(|path| modules.get(path).cloned())
        .collect()
}

/// True for `pub` exactly — `pub(crate)` / `pub(super)` cannot leave the crate.
/// Operation: visibility match, no own calls.
fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

/// True when `file` is a library crate root (`…/src/lib.rs`).
/// Operation: suffix test, no own calls.
fn is_lib_root(file: &str) -> bool {
    let norm = file.replace('\\', "/");
    norm == "src/lib.rs" || norm.ends_with("/src/lib.rs")
}

/// The module path of a source file relative to its package's `src/`
/// (`src/a/b.rs` → `["a","b"]`, `src/a/mod.rs` → `["a"]`, root → `[]`), or
/// `None` when the layout is not recognised — those files are treated as
/// reachable so an unusual layout never manufactures a finding.
/// Operation: path splitting, no own calls.
fn module_path_of(file: &str) -> Option<Vec<String>> {
    let norm = file.replace('\\', "/");
    let idx = norm.find("src/")?;
    // Only `src/` at the start or right after a directory separator.
    if idx != 0 && !norm[..idx].ends_with('/') {
        return None;
    }
    let rel = norm[idx + 4..].strip_suffix(".rs")?;
    let mut segments: Vec<String> = rel.split('/').map(str::to_string).collect();
    match segments.last().map(String::as_str) {
        Some("lib") | Some("main") if segments.len() == 1 => return Some(Vec::new()),
        Some("mod") => {
            segments.pop();
        }
        _ => {}
    }
    Some(segments)
}
