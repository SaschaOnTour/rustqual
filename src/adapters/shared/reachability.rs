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
    /// `(file, name)` pairs re-exported via `pub use path::Name`.
    reexported_items: HashSet<(String, String)>,
    /// Files whose items are all re-exported via `pub use path::*`.
    glob_reexported_files: HashSet<String>,
    /// Files the layout derivation did not recognise — treated as reachable.
    unknown_files: HashSet<String>,
}

impl ExternalReach {
    /// True when `name` in `file` can be named from outside the crate.
    /// Integration: combines the layout, visibility and re-export facts.
    pub(crate) fn is_externally_reachable(&self, file: &str, name: &str) -> bool {
        if self.unknown_files.contains(file)
            || self
                .reexported_items
                .contains(&(file.to_string(), name.to_string()))
        {
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
        reexported_items: resolve_reexports(&facts.reexports, &modules),
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
    /// `(module key, name)` re-exported by a `pub use …::Name;` — keyed by
    /// the source module, so one crate.s re-export cannot excuse a same-named
    /// function somewhere else.
    reexports: HashSet<(String, String)>,
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
                collect_use(&u.tree, file, mod_path, &mut Vec::new(), facts);
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
    file: &str,
    mod_path: &[String],
    prefix: &mut Vec<String>,
    facts: &mut FileFacts,
) {
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            collect_use(&p.tree, file, mod_path, prefix, facts);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            record_reexport(file, mod_path, prefix, &n.ident.to_string(), facts);
        }
        syn::UseTree::Rename(r) => {
            // The *source* name is what keeps the original item public.
            record_reexport(file, mod_path, prefix, &r.ident.to_string(), facts);
        }
        syn::UseTree::Glob(_) => {
            if let Some(key) = module_key(file, &resolve_use_prefix(mod_path, prefix)) {
                facts.glob_uses.insert(key);
            }
        }
        syn::UseTree::Group(g) => g
            .items
            .iter()
            .for_each(|t| collect_use(t, file, mod_path, &mut prefix.clone(), facts)),
    }
}

/// Turn a `use` prefix into an absolute module path, resolving the
/// `crate::`/`self::` heads against the current scope.
/// Operation: prefix normalisation, no own calls.
fn resolve_use_prefix(mod_path: &[String], prefix: &[String]) -> Vec<String> {
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
    out
}

/// Record a concrete `pub use …::Name` against its SOURCE module, so the
/// re-export excuses that one item — not every same-named function in the
/// workspace. Operation: key build + insert.
fn record_reexport(
    file: &str,
    mod_path: &[String],
    prefix: &[String],
    name: &str,
    facts: &mut FileFacts,
) {
    if let Some(key) = module_key(file, &resolve_use_prefix(mod_path, prefix)) {
        facts.reexports.insert((key, name.to_string()));
    }
}

/// Map module path → file, for every file whose layout we understand.
/// Operation: path derivation over the parsed set.
fn module_index(parsed: &[(String, String, syn::File)]) -> HashMap<String, String> {
    parsed
        .iter()
        .filter_map(|(file, _, _)| {
            let path = module_path_of(file)?;
            Some((module_key(file, &path)?, file.clone()))
        })
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
            let Some(key) = module_key(parent, &child_path) else {
                continue;
            };
            if let Some(child_file) = modules.get(&key) {
                changed |= reachable.insert(child_file.clone());
            }
        }
    }
    reachable
}

/// Turn `(source module key, name)` re-exports into `(file, name)` pairs, so a
/// re-export excuses exactly the item it names — not every same-named function
/// in the workspace. A key that resolves to no known file is dropped.
/// Operation: lookup + projection.
fn resolve_reexports(
    reexports: &HashSet<(String, String)>,
    modules: &HashMap<String, String>,
) -> HashSet<(String, String)> {
    reexports
        .iter()
        .filter_map(|(key, name)| modules.get(key).map(|f| (f.clone(), name.clone())))
        .collect()
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

/// The package prefix of a source file — everything before its `src/`, so two
/// workspace crates that both contain `src/api.rs` never share a module key.
/// Operation: path splitting, no own calls.
fn package_of(file: &str) -> Option<String> {
    split_at_src(file).map(|(package, _)| package)
}

/// Split a source path at its package's `src/` into `(package prefix, path
/// below src/)`. `None` when the layout is not recognised — the single place
/// that decides what "inside a package" means, so the package prefix and the
/// module path can never disagree.
/// Operation: path splitting, no own calls.
fn split_at_src(file: &str) -> Option<(String, String)> {
    let norm = file.replace('\\', "/");
    let idx = norm.find("src/")?;
    // Only `src/` at the start or right after a directory separator.
    if idx != 0 && !norm[..idx].ends_with('/') {
        return None;
    }
    Some((norm[..idx].to_string(), norm[idx + 4..].to_string()))
}

/// A module's index key: its package prefix plus its module path, so
/// `crates/a/src/api.rs` and `crates/b/src/api.rs` stay distinct.
/// Operation: string join, no own calls.
fn module_key(file: &str, path: &[String]) -> Option<String> {
    Some(format!("{}|{}", package_of(file)?, path.join("::")))
}

/// The module path of a source file relative to its package's `src/`
/// (`src/a/b.rs` → `["a","b"]`, `src/a/mod.rs` → `["a"]`, root → `[]`), or
/// `None` when the layout is not recognised — those files are treated as
/// reachable so an unusual layout never manufactures a finding.
/// Operation: path splitting, no own calls.
fn module_path_of(file: &str) -> Option<Vec<String>> {
    let (_, below) = split_at_src(file)?;
    let rel = below.strip_suffix(".rs")?;
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
