//! Can an item be named from *outside* its crate?
//!
//! `// qual:api` exists to excuse an entry point whose callers live outside the
//! analysed code. That excuse only makes sense for items an outside consumer
//! can actually reach: `pub`, behind an unbroken chain of `pub mod`, in a
//! library crate. An item behind a private `mod` is unreachable no matter how
//! many `pub` keywords it carries — a `qual:api` there is a category error.
//!
//! The derivation is pure `.rs`-set analysis (no `Cargo.toml`, matching the
//! rest of rustqual): file paths and inline `mod` blocks give module paths
//! (`paths`), one walk gathers the visibility and `pub use` facts (`collect`),
//! and this module closes over them — `pub mod` links, glob preludes, and
//! re-export chains including renames.
//!
//! **The bias is deliberate: every uncertainty resolves to *reachable*.** The
//! consumer reports a marker as "never applied" when an item is unreachable,
//! so a wrong "unreachable" would demand deleting a marker that is still doing
//! its job. Missing a re-export costs a finding; inventing one costs trust.

mod collect;
mod paths;

use std::collections::{HashMap, HashSet};

use collect::{collect_file_facts, FileFacts, GlobUse, ReexportUse};
use paths::{is_lib_root, module_key, module_path_of};

/// Which items a consumer outside the crate can name.
pub(crate) struct ExternalReach {
    /// Files whose module path is reachable through `pub mod` links.
    reachable_files: HashSet<String>,
    /// `(file, fn-name)` pairs that are `pub` through their inline-mod chain.
    pub_items: HashSet<(String, String)>,
    /// `(file, name)` pairs a `pub use path::Name` exposes.
    reexported_items: HashSet<(String, String)>,
    /// Files whose `pub` items are all exposed by a `pub use path::*`.
    glob_reexported_files: HashSet<String>,
    /// Files the layout derivation did not recognise — treated as reachable.
    unknown_files: HashSet<String>,
}

impl ExternalReach {
    /// True when `name` in `file` can be named from outside the crate.
    /// Integration: combines the layout, visibility and re-export facts.
    pub(crate) fn is_externally_reachable(&self, file: &str, name: &str) -> bool {
        let item = (file.to_string(), name.to_string());
        if self.unknown_files.contains(file) || self.reexported_items.contains(&item) {
            return true;
        }
        let is_pub = self.pub_items.contains(&item);
        if self.glob_reexported_files.contains(file) {
            return is_pub;
        }
        is_pub && self.reachable_files.contains(file)
    }
}

/// Build the reachability facts for the whole parsed set.
/// Integration: per-file facts, then the module / glob / re-export closures.
pub(crate) fn compute_external_reach(parsed: &[(String, String, syn::File)]) -> ExternalReach {
    let mut facts = FileFacts::default();
    parsed
        .iter()
        .for_each(|(file, _, syntax)| collect_file_facts(file, syntax, &mut facts));
    let modules = module_index(parsed, &facts);
    let reachable_files = walk_reachable(parsed, &modules, &facts);
    let glob_reexported_files = resolve_globs(&facts.globs, &modules, &reachable_files);
    let reexported_items = resolve_reexports(
        &facts.reexports,
        &modules,
        &reachable_files,
        &glob_reexported_files,
    );
    ExternalReach {
        reachable_files,
        pub_items: facts.pub_items,
        reexported_items,
        glob_reexported_files,
        unknown_files: unrecognised_files(parsed),
    }
}

/// Files whose layout the derivation does not understand — treated as
/// reachable, so an odd layout never manufactures a finding.
/// Operation: filter over the parsed set.
fn unrecognised_files(parsed: &[(String, String, syn::File)]) -> HashSet<String> {
    parsed
        .iter()
        .map(|(f, _, _)| f.clone())
        .filter(|f| module_path_of(f).is_none())
        .collect()
}

/// Map module key → the file holding that module, for files and inline blocks.
/// Operation: path derivation over the parsed set plus the inline blocks.
fn module_index(
    parsed: &[(String, String, syn::File)],
    facts: &FileFacts,
) -> HashMap<String, String> {
    let mut index: HashMap<String, String> = parsed
        .iter()
        .filter_map(|(file, _, _)| {
            let path = module_path_of(file)?;
            Some((module_key(file, &path)?, file.clone()))
        })
        .collect();
    // An inline `mod x { … }` lives in its declaring file, so a re-export
    // naming it resolves to that file.
    facts.inline_modules.iter().for_each(|(key, file)| {
        index.entry(key.clone()).or_insert_with(|| file.clone());
    });
    index
}

/// Files reachable from a library crate root through `pub mod` links.
/// Integration: seeds the roots, then expands to a fixpoint.
fn walk_reachable(
    parsed: &[(String, String, syn::File)],
    modules: &HashMap<String, String>,
    facts: &FileFacts,
) -> HashSet<String> {
    let mut reachable: HashSet<String> = parsed
        .iter()
        .map(|(f, _, _)| f.clone())
        .filter(|f| is_lib_root(f))
        .collect();
    let mut changed = true;
    while changed {
        let next: Vec<String> = facts
            .pub_mod_links
            .iter()
            .filter(|(parent, _, _)| reachable.contains(parent))
            .filter_map(|(parent, parent_path, child)| {
                child_file(parent, parent_path, child, modules)
            })
            .collect();
        changed = next
            .into_iter()
            .fold(false, |acc, f| acc | reachable.insert(f));
    }
    reachable
}

/// The file implementing `child`, declared in `parent` inside the scope
/// `parent_path`. The scope matters: `pub mod outer { pub mod inner; }`
/// implements `outer::inner`, not a top-level `inner`.
/// Operation: path extension + index lookup.
fn child_file(
    parent: &str,
    parent_path: &[String],
    child: &str,
    modules: &HashMap<String, String>,
) -> Option<String> {
    let mut path = parent_path.to_vec();
    path.push(child.to_string());
    modules.get(&module_key(parent, &path)?).cloned()
}

/// Files whose `pub` surface a glob exposes. Seeded from globs written in a
/// reachable file, then followed through glob-exposed façades — the prelude
/// shape `lib → facade::* → deep::*` needs every hop.
/// Integration: seed + fixpoint over the glob edges.
fn resolve_globs(
    globs: &[GlobUse],
    modules: &HashMap<String, String>,
    reachable_files: &HashSet<String>,
) -> HashSet<String> {
    let targets = |g: &GlobUse| -> Vec<String> {
        g.targets
            .iter()
            .filter_map(|k| modules.get(k).cloned())
            .collect()
    };
    let mut queue: Vec<String> = globs
        .iter()
        .filter(|g| reachable_files.contains(&g.file))
        .flat_map(targets)
        .collect();
    let mut exposed: HashSet<String> = HashSet::new();
    while let Some(file) = queue.pop() {
        if !exposed.insert(file.clone()) {
            continue;
        }
        queue.extend(globs.iter().filter(|g| g.file == file).flat_map(targets));
    }
    exposed
}

/// `(file, name)` pairs a `pub use` exposes. Seeded from re-exports written
/// where an outside consumer can see them, then followed hop by hop: each hop
/// matches the *exported* name of the previous one and carries the item's own
/// *source* name forward, so a rename mid-chain keeps the link.
/// Integration: seed + fixpoint over the re-export edges.
fn resolve_reexports(
    reexports: &[ReexportUse],
    modules: &HashMap<String, String>,
    reachable_files: &HashSet<String>,
    glob_files: &HashSet<String>,
) -> HashSet<(String, String)> {
    let hops = |r: &ReexportUse| -> Vec<(String, String)> {
        r.targets
            .iter()
            .filter_map(|k| modules.get(k).cloned())
            .map(|f| (f, r.source.clone()))
            .collect()
    };
    let mut queue: Vec<(String, String)> = reexports
        .iter()
        .filter(|r| reachable_files.contains(&r.file) || glob_files.contains(&r.file))
        .flat_map(hops)
        .collect();
    let mut exposed: HashSet<(String, String)> = HashSet::new();
    while let Some((file, name)) = queue.pop() {
        if !exposed.insert((file.clone(), name.clone())) {
            continue;
        }
        queue.extend(
            reexports
                .iter()
                .filter(|r| r.file == file && r.exported == name)
                .flat_map(hops),
        );
    }
    exposed
}
