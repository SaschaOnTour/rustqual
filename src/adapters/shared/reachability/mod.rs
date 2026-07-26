//! Can an item be named from *outside* its crate?
//!
//! `// qual:api` exists to excuse an entry point whose callers live outside the
//! analysed code. That excuse only makes sense for items an outside consumer
//! can actually reach: `pub`, behind an unbroken chain of `pub mod`, in a
//! library crate. An item behind a private `mod` is unreachable no matter how
//! many `pub` keywords it carries — a `qual:api` there is a category error.
//!
//! The derivation is pure `.rs`-set analysis (no `Cargo.toml`, matching the
//! rest of rustqual): one walk starts at every crate root
//! (`shared::crate_roots`), follows `mod` declarations through the shared
//! layout rules (`shared::child_paths`) and gathers the visibility and
//! `pub use` facts (`collect`); this module closes over them — `pub mod`
//! links, glob preludes, and re-export chains including renames.
//!
//! **The bias is deliberate: every uncertainty resolves to *reachable*.** The
//! consumer reports a marker as "never applied" when an item is unreachable,
//! so a wrong "unreachable" would demand deleting a marker that is still doing
//! its job. Missing a re-export costs a finding; inventing one costs trust.
//!
//! **Known limit, in that safe direction (issue #40):** items are identified
//! by `(file, name)`. Two same-named functions in *different inline modules of
//! one file* — say a `pub` one in `mod shown` and a private one in
//! `mod hidden` — are indistinguishable, so the public one makes the private
//! one look reachable and a stale marker on it goes unreported. Fixing this
//! needs a qualified item key threaded through `DeclaredFunction` and the
//! analyzers that share it; that is its own piece of work, and its payoff is
//! capped anyway because call sites are recorded by last path segment (see
//! `dry::call_targets`), so declarations alone cannot attribute a call.

mod collect;

use std::collections::{HashMap, HashSet};

use collect::{walk_crate_tree, GlobUse, ModuleTree, ReexportUse};

/// Which items a consumer outside the crate can name.
pub(crate) struct ExternalReach {
    /// Files whose module chain is `pub` all the way to a library root.
    reachable_files: HashSet<String>,
    /// `(file, fn-name)` pairs that are `pub` through their whole chain.
    pub_items: HashSet<(String, String)>,
    /// `(file, name)` pairs a `pub use path::Name` exposes.
    reexported_items: HashSet<(String, String)>,
    /// Files whose `pub` items are all exposed by a `pub use path::*`.
    glob_reexported_files: HashSet<String>,
    /// Files the walk never reached from any library root — no module tree
    /// claims them, so nothing can be concluded and they count as reachable.
    unknown_files: HashSet<String>,
}

impl ExternalReach {
    /// True when `name` in `file` can be named from outside the crate.
    /// Integration: combines the tree, visibility and re-export facts.
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
/// Integration: tree walk, then the glob / re-export closures.
pub(crate) fn compute_external_reach(parsed: &[(String, String, syn::File)]) -> ExternalReach {
    let tree = walk_crate_tree(parsed);
    let glob_reexported_files = resolve_globs(&tree.globs, &tree.modules, &tree.reachable_files);
    let reexported_items = resolve_reexports(
        &tree.reexports,
        &tree.modules,
        &tree.reachable_files,
        &glob_reexported_files,
    );
    ExternalReach {
        unknown_files: unwalked_files(parsed, &tree),
        reachable_files: tree.reachable_files,
        pub_items: tree.pub_items,
        reexported_items,
        glob_reexported_files,
    }
}

/// Files no crate-root walk ever reached — a binary-only tree, an excluded
/// layout, a file outside any `mod` chain. Nothing can be concluded about
/// them, so they count as reachable rather than manufacturing a finding.
/// Operation: set difference over the parsed set.
fn unwalked_files(parsed: &[(String, String, syn::File)], tree: &ModuleTree) -> HashSet<String> {
    let walked: HashSet<&String> = tree.modules.values().collect();
    parsed
        .iter()
        .map(|(f, _, _)| f.clone())
        .filter(|f| !walked.contains(f))
        .collect()
}

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
        .filter(|g| g.scope_is_pub && reachable_files.contains(&g.file))
        .flat_map(targets)
        .collect();
    let mut exposed: HashSet<String> = HashSet::new();
    while let Some(file) = queue.pop() {
        if !exposed.insert(file.clone()) {
            continue;
        }
        queue.extend(
            globs
                .iter()
                .filter(|g| g.scope_is_pub && g.file == file)
                .flat_map(targets),
        );
    }
    exposed
}

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
        .filter(|r| r.scope_is_pub)
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
                .filter(|r| r.scope_is_pub && r.file == file && r.exported == name)
                .flat_map(hops),
        );
    }
    exposed
}
