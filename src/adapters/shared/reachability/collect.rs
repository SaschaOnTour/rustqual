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
use super::super::crate_roots::{crate_root_of, CrateRootKind};

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
    /// `(file, owner, method)`. A method is reachable exactly when its owner
    /// is, which the `pub mod` chain alone cannot answer: the common façade
    /// puts a type in a private module and re-exports it at the crate root, so
    /// the file is in no public chain while the type — and therefore every
    /// method on it — is callable from outside.
    pub methods: Vec<(String, String, String)>,
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
    /// How much of `mod_path` was already established when this file was
    /// entered — the rest is the inline `mod {}` chain within it, which the
    /// resolver needs to know where a child module's files live.
    file_depth: usize,
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
    roots_to_walk(&refs)
        .into_iter()
        .for_each(|(root, exposed)| {
            walk.file(Scope {
                file: root,
                root,
                mod_path: &[],
                file_depth: 0,
                chain_is_pub: exposed,
                inline_is_pub: true,
            });
        });
    walk.tree
}

/// The roots to walk, each with whether its tree is externally exposed.
/// Library roots are; binary roots are walked only so their files count as
/// known, since nothing in a binary is nameable from outside. Which files are
/// roots at all is the shared [`crate_root_of`] rule — a file it does not
/// claim (say `src/bin/tools/helper.rs`) starts no walk and stays unknown.
/// Operation: filter over the parsed paths, own call hidden in the closure.
fn roots_to_walk<'a>(refs: &ParsedRefs<'a>) -> Vec<(&'a str, bool)> {
    refs.iter()
        .map(|(p, _)| *p)
        .filter_map(|p| crate_root_of(p).map(|(_, kind)| (p, kind == CrateRootKind::Lib)))
        .collect()
}

impl Walk<'_> {
    /// Walk one file: register it, then its items.
    /// The visit key carries the crate root, matching a module's identity: one
    /// file can be pulled into two crates at the same logical path, and each
    /// crate decides its visibility for itself.
    /// Integration: registration + item traversal.
    // qual:recursive
    fn file(&mut self, scope: Scope<'_>) {
        if !self.visited.insert(format!(
            "{}|{}",
            module_key(scope.root, scope.mod_path),
            scope.file
        )) {
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
                syn::Item::Mod(m) => self.module(scope, m),
                // A `pub use` inside a private module is collected too — whether
                // it exposes anything is decided later, from the scope's own
                // visibility.
                syn::Item::Use(u) if is_pub(&u.vis) => {
                    collect_use(&u.tree, scope, &mut Vec::new(), &mut self.tree);
                }
                other => self.named_item(scope, other),
            }
        }
    }

    /// A `pub` item an outside consumer could name. Types and constants count
    /// alongside functions: `qual:api` is verified against this set and, since
    /// DRY-006, may sit on any of them.
    /// Operation: shape lookup + insert, own call hidden in the closure.
    fn named_item(&mut self, scope: Scope<'_>, item: &syn::Item) {
        let named = public_name(item).filter(|_| scope.inline_is_pub);
        if let Some(name) = named {
            self.tree.pub_items.insert((scope.file.to_string(), name));
        }
        self.associated_names(scope, item);
    }

    /// Methods, which are `ImplItem`s and `TraitItem`s rather than items — an
    /// item-level walk never sees them, and `qual:api` sits on a method more
    /// often than on anything else.
    ///
    /// A method's own `pub` is recorded without checking that its type is
    /// nameable. That over-approximates, which is this module's stated
    /// direction: an item wrongly called reachable costs a missed finding, one
    /// wrongly called unreachable accuses an author of writing a marker that
    /// could never work. Integration: shape dispatch.
    fn associated_names(&mut self, scope: Scope<'_>, item: &syn::Item) {
        match item {
            syn::Item::Impl(i) => {
                let owner = self_type_name(&i.self_ty);
                let names = impl_method_names(&i.items, i.trait_.is_some());
                self.record_methods(scope, owner, names);
            }
            syn::Item::Trait(t) => {
                let owner = Some(t.ident.to_string());
                self.record_methods(scope, owner, trait_method_names(&t.items));
            }
            _ => {}
        }
    }

    /// Operation: bulk insert against the owning type, no own calls.
    fn record_methods(&mut self, scope: Scope<'_>, owner: Option<String>, names: Vec<String>) {
        let file = scope.file.to_string();
        owner.into_iter().for_each(|owner| {
            self.tree.methods.extend(
                names
                    .iter()
                    .map(|name| (file.clone(), owner.clone(), name.clone())),
            );
        });
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
                let inline = &scope.mod_path[scope.file_depth..];
                if let Some(child) = self.resolver.resolve(scope.file, inline, m) {
                    self.file(Scope {
                        file: &child,
                        root: scope.root,
                        mod_path: &nested,
                        // A new file starts a fresh inline chain.
                        file_depth: nested.len(),
                        inline_is_pub: true,
                        chain_is_pub,
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

/// The name a `pub` item publishes, or `None` when it is not public or has no
/// nameable identity (an `impl` block, a `macro_rules!`, …). Which shapes carry
/// a name is the shared table's answer, so this cannot forget one.
/// Operation: shape lookup + visibility filter, own calls in the closure.
fn public_name(item: &syn::Item) -> Option<String> {
    super::super::item_shape::item_ident_and_vis(item)
        .filter(|(_, vis)| is_pub(vis))
        .map(|(ident, _)| ident.to_string())
}

/// The methods of an `impl` block an outside consumer could name. In an
/// inherent impl that means the `pub` ones; in a trait impl it means all of
/// them, since they carry no visibility of their own and are reached through
/// the trait.
/// Operation: filter over the associated items, no own calls.
fn impl_method_names(items: &[syn::ImplItem], is_trait_impl: bool) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item {
            syn::ImplItem::Fn(f) => Some(f),
            _ => None,
        })
        .filter(|f| is_trait_impl || is_pub(&f.vis))
        .map(|f| f.sig.ident.to_string())
        .collect()
}

/// A trait's methods, which carry no visibility of their own — they are as
/// public as the trait.
/// Operation: filter over the associated items, no own calls.
fn trait_method_names(items: &[syn::TraitItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item {
            syn::TraitItem::Fn(f) => Some(f.sig.ident.to_string()),
            _ => None,
        })
        .collect()
}

/// The name of the type an `impl` block is for — its final path segment, the
/// same grain everything else here works at.
/// Operation: shape lookup, no own calls.
fn self_type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// Operation: visibility match, no own calls.
fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}
