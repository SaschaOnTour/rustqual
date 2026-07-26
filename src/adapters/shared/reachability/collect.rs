//! Walking the crate's module tree to gather the facts reachability needs.
//!
//! The walk starts at each library root and follows `mod` declarations through
//! [`ChildPathResolver`] — the shared owner of rustc's file-layout rules
//! (`#[path]`, `{dir}/{name}.rs`, `{dir}/{name}/mod.rs`). Because the tree is
//! *walked* rather than derived from file paths, a module's logical path and
//! the file implementing it always agree: a `#[path]` redirect, a nested
//! inline module and their children all fall out of the same traversal.

use std::collections::{HashMap, HashSet};

use super::super::child_paths::{ChildPathResolver, ParsedRefs};

/// One `pub use path::Name` (or `… as Alias`). `targets` holds every module
/// key the path might mean — an unprefixed path can be local or crate-root
/// under uniform paths, and guessing one would drop valid re-exports.
pub(super) struct ReexportUse {
    pub file: String,
    /// Whether the inline-module chain around the `pub use` is public. The
    /// declaring file can be reachable while the `mod hidden { … }` holding
    /// the re-export is not — then nothing is exposed.
    pub scope_is_pub: bool,
    pub targets: Vec<String>,
    /// The name the re-export publishes (the alias, when renamed).
    pub exported: String,
    /// The name the item carries in its own module.
    pub source: String,
}

/// One `pub use path::*`.
pub(super) struct GlobUse {
    pub file: String,
    /// See `ReexportUse::scope_is_pub`.
    pub scope_is_pub: bool,
    pub targets: Vec<String>,
}

/// Everything the walk observed.
#[derive(Default)]
pub(super) struct ModuleTree {
    /// `(file, fn-name)` for functions `pub` through their whole chain.
    pub pub_items: HashSet<(String, String)>,
    /// Logical module key → the file implementing it.
    pub modules: HashMap<String, String>,
    /// Files whose module chain is `pub` all the way to a library root.
    pub reachable_files: HashSet<String>,
    pub reexports: Vec<ReexportUse>,
    pub globs: Vec<GlobUse>,
}

/// Where the walk currently is.
#[derive(Clone, Copy)]
struct Scope<'a> {
    /// The file being walked.
    file: &'a str,
    /// The crate root this file belongs to — module keys are prefixed with it
    /// so two workspace crates never share a logical path.
    root: &'a str,
    /// Logical module path of the current (possibly inline) scope.
    mod_path: &'a [String],
    /// Whether every `mod` from the crate root down to here is `pub` — this
    /// decides whether the *file* is externally reachable.
    chain_is_pub: bool,
    /// Whether the inline-module chain *within this file* is `pub`. An item
    /// can be `pub` in its file while the file itself sits behind a private
    /// `mod`; the two are judged separately, and combined by the caller.
    inline_is_pub: bool,
}

/// The walk's shared state: the parsed ASTs, the shared module resolver, the
/// scopes already visited, and the facts gathered so far. Bundled so the
/// traversal functions carry one context instead of five parameters.
struct Walk<'a> {
    asts: HashMap<&'a str, &'a syn::File>,
    resolver: ChildPathResolver<'a>,
    visited: HashSet<String>,
    tree: ModuleTree,
}

/// Walk every crate's module tree.
/// Integration: resolver setup + per-root traversal.
pub(super) fn walk_crate_tree(parsed: &[(String, String, syn::File)]) -> ModuleTree {
    let refs: Vec<(&str, &syn::File)> = parsed.iter().map(|(p, _, f)| (p.as_str(), f)).collect();
    let mut walk = Walk {
        asts: refs.iter().copied().collect(),
        resolver: ChildPathResolver::from_parsed(&refs),
        visited: HashSet::new(),
        tree: ModuleTree::default(),
    };
    crate_roots(&refs).into_iter().for_each(|(root, exposed)| {
        walk.file(Scope {
            file: root,
            root,
            mod_path: &[],
            chain_is_pub: exposed,
            inline_is_pub: true,
        });
    });
    walk.tree
}

/// The roots to walk, each with whether its tree is externally exposed.
/// Library roots are; binary roots are walked only so their files count as
/// known, since nothing in a binary is nameable from outside.
/// Operation: filter over the parsed paths, no own calls.
fn crate_roots<'a>(refs: &ParsedRefs<'a>) -> Vec<(&'a str, bool)> {
    refs.iter()
        .map(|(p, _)| *p)
        .filter_map(|p| match (is_lib_root(p), is_bin_root(p)) {
            (true, _) => Some((p, true)),
            (_, true) => Some((p, false)),
            _ => None,
        })
        .collect()
}

impl Walk<'_> {
    /// Walk one file: register it, then its items.
    /// Integration: registration + item traversal.
    // qual:recursive
    fn file(&mut self, scope: Scope<'_>) {
        if !self
            .visited
            .insert(format!("{}|{}", scope.file, scope.mod_path.join("::")))
        {
            return;
        }
        register_module(scope, &mut self.tree);
        let Some(syntax) = self.asts.get(scope.file).copied() else {
            return;
        };
        self.items(scope, &syntax.items);
    }

    /// The items of one scope.
    /// Integration: per-item delegation.
    // qual:recursive
    fn items(&mut self, scope: Scope<'_>, items: &[syn::Item]) {
        for item in items {
            match item {
                syn::Item::Fn(f) if scope.inline_is_pub && is_pub(&f.vis) => {
                    self.tree
                        .pub_items
                        .insert((scope.file.to_string(), f.sig.ident.to_string()));
                }
                syn::Item::Mod(m) => self.module(scope, m),
                // A `pub use` inside a private module is collected too — whether
                // it exposes anything is decided later, from the scope's own
                // visibility.
                syn::Item::Use(u) if is_pub(&u.vis) => {
                    collect_use(&u.tree, scope, &mut Vec::new(), &mut self.tree);
                }
                _ => {}
            }
        }
    }

    /// One `mod` item: an inline block continues in the same file, an
    /// out-of-line one resolves to its implementing file through the shared
    /// resolver. Integration: shape dispatch + recursion.
    fn module(&mut self, scope: Scope<'_>, m: &syn::ItemMod) {
        let mut nested = scope.mod_path.to_vec();
        nested.push(m.ident.to_string());
        let chain_is_pub = scope.chain_is_pub && is_pub(&m.vis);
        match &m.content {
            Some((_, inner)) => {
                let inner_scope = Scope {
                    mod_path: &nested,
                    chain_is_pub,
                    inline_is_pub: scope.inline_is_pub && is_pub(&m.vis),
                    ..scope
                };
                register_module(inner_scope, &mut self.tree);
                self.items(inner_scope, inner);
            }
            None => {
                if let Some(child) = self.resolver.resolve(scope.file, m) {
                    self.file(Scope {
                        file: &child,
                        root: scope.root,
                        mod_path: &nested,
                        chain_is_pub,
                        // A new file starts a fresh inline chain.
                        inline_is_pub: true,
                    });
                }
            }
        }
    }
}

/// Record a module's key → file mapping and, when its chain is public, that
/// the file is externally reachable.
/// Operation: two inserts, no own calls.
fn register_module(scope: Scope<'_>, tree: &mut ModuleTree) {
    tree.modules
        .entry(module_key(scope.root, scope.mod_path))
        .or_insert_with(|| scope.file.to_string());
    if scope.chain_is_pub {
        tree.reachable_files.insert(scope.file.to_string());
    }
}

/// Record what a `pub use` tree exposes: named items (with their alias, if
/// any) and glob targets.
/// Integration: use-tree recursion accumulating the path prefix.
// qual:recursive
fn collect_use(
    tree_node: &syn::UseTree,
    scope: Scope<'_>,
    prefix: &mut Vec<String>,
    tree: &mut ModuleTree,
) {
    match tree_node {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            collect_use(&p.tree, scope, prefix, tree);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let name = n.ident.to_string();
            push_reexport(scope, prefix, &name, &name, tree);
        }
        // `use hidden::entry as public_entry` publishes `public_entry` while
        // the item is still `entry` in its own module — the chain needs both.
        syn::UseTree::Rename(r) => {
            push_reexport(
                scope,
                prefix,
                &r.rename.to_string(),
                &r.ident.to_string(),
                tree,
            );
        }
        syn::UseTree::Glob(_) => tree.globs.push(GlobUse {
            file: scope.file.to_string(),
            scope_is_pub: scope.inline_is_pub,
            targets: target_keys(scope, prefix),
        }),
        syn::UseTree::Group(g) => g
            .items
            .iter()
            .for_each(|t| collect_use(t, scope, &mut prefix.clone(), tree)),
    }
}

/// Record one named re-export against every module its path might mean.
/// Operation: struct construction, key building delegated.
fn push_reexport(
    scope: Scope<'_>,
    prefix: &[String],
    exported: &str,
    source: &str,
    tree: &mut ModuleTree,
) {
    tree.reexports.push(ReexportUse {
        file: scope.file.to_string(),
        scope_is_pub: scope.inline_is_pub,
        targets: target_keys(scope, prefix),
        exported: exported.to_string(),
        source: source.to_string(),
    });
}

/// Every module key a `use` prefix might denote. `crate::`/`self::`/`super::`
/// are unambiguous; an unprefixed path can mean the current module's child or
/// a crate-root item (uniform paths), so both are offered and whichever
/// resolves wins — picking one would silently drop valid re-exports.
/// Operation: candidate paths → keys.
fn target_keys(scope: Scope<'_>, prefix: &[String]) -> Vec<String> {
    candidate_paths(scope.mod_path, prefix)
        .iter()
        .map(|p| module_key(scope.root, p))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

/// The absolute module paths a `use` prefix may resolve to.
/// Operation: head-segment dispatch, no own calls.
fn candidate_paths(mod_path: &[String], prefix: &[String]) -> Vec<Vec<String>> {
    let local = |rest: &[String]| {
        let mut p = mod_path.to_vec();
        p.extend_from_slice(rest);
        p
    };
    match prefix.first().map(String::as_str) {
        Some("crate") => vec![prefix[1..].to_vec()],
        Some("self") => vec![local(&prefix[1..])],
        Some("super") => {
            let climbs = prefix.iter().take_while(|s| s.as_str() == "super").count();
            let mut climbed = mod_path.to_vec();
            climbed.truncate(mod_path.len().saturating_sub(climbs));
            climbed.extend_from_slice(&prefix[climbs..]);
            vec![climbed]
        }
        _ => vec![local(prefix), prefix.to_vec()],
    }
}

/// A module's identity: its crate root plus its logical path, so two workspace
/// crates with the same module names stay distinct.
/// Operation: string join, no own calls.
fn module_key(root: &str, path: &[String]) -> String {
    format!("{root}|{}", path.join("::"))
}

/// Operation: suffix test, no own calls.
fn is_lib_root(file: &str) -> bool {
    let norm = file.replace('\\', "/");
    norm == "src/lib.rs" || norm.ends_with("/src/lib.rs")
}

/// Operation: suffix tests, no own calls.
fn is_bin_root(file: &str) -> bool {
    let norm = file.replace('\\', "/");
    norm == "src/main.rs"
        || norm.ends_with("/src/main.rs")
        || norm.starts_with("src/bin/")
        || norm.contains("/src/bin/")
}

/// Operation: visibility match, no own calls.
fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}
