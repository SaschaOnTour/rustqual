//! One pass over each file, recording the facts reachability is derived from:
//! which functions are `pub` through their whole inline-module chain, which
//! `mod` links stay public, which inline modules exist, and what every
//! `pub use` exposes.

use std::collections::HashSet;

use super::paths::{is_pub, module_key, module_path_of};

/// One `pub use path::Name` (or `… as Alias`). `targets` holds every module
/// key the path might mean — an unprefixed path can be local or crate-root
/// under uniform paths, and guessing one would drop valid re-exports.
pub(super) struct ReexportUse {
    pub file: String,
    pub targets: Vec<String>,
    /// The name the re-export publishes (the alias, when renamed).
    pub exported: String,
    /// The name the item carries in its own module.
    pub source: String,
}

/// One `pub use path::*`.
pub(super) struct GlobUse {
    pub file: String,
    pub targets: Vec<String>,
}

/// Per-file syntax facts gathered in one pass.
#[derive(Default)]
pub(super) struct FileFacts {
    /// `(file, fn-name)` for functions `pub` through their inline-mod chain.
    pub pub_items: HashSet<(String, String)>,
    /// `(file, parent module path, child name)` for `pub mod child;`. The
    /// parent path matters: `pub mod outer { pub mod inner; }` implements
    /// `outer::inner`, not a top-level `inner`.
    pub pub_mod_links: HashSet<(String, Vec<String>, String)>,
    /// `(module key, declaring file)` for inline `mod x { … }` blocks, so a
    /// re-export can name a module that has no file of its own.
    pub inline_modules: Vec<(String, String)>,
    pub reexports: Vec<ReexportUse>,
    pub globs: Vec<GlobUse>,
}

/// Where the walker currently is: the file, the module path of the enclosing
/// (possibly inline) scope, and whether that whole chain is `pub`.
#[derive(Clone, Copy)]
struct Scope<'a> {
    file: &'a str,
    mod_path: &'a [String],
    chain_is_pub: bool,
}

/// Walk one file's items into `facts`.
/// Operation: seeds the scope and delegates to the item walker.
pub(super) fn collect_file_facts(file: &str, syntax: &syn::File, facts: &mut FileFacts) {
    let base = module_path_of(file).unwrap_or_default();
    let scope = Scope {
        file,
        mod_path: &base,
        chain_is_pub: true,
    };
    walk_items(scope, &syntax.items, facts);
}

/// Every item in one scope.
/// Integration: per-item delegation.
// qual:recursive
fn walk_items(scope: Scope<'_>, items: &[syn::Item], facts: &mut FileFacts) {
    items.iter().for_each(|item| walk_item(scope, item, facts));
}

/// One item: a `pub` fn is recorded, a `mod` recurses, a `pub use` is parsed.
/// Integration: shape dispatch, each arm delegated.
fn walk_item(scope: Scope<'_>, item: &syn::Item, facts: &mut FileFacts) {
    match item {
        syn::Item::Fn(f) if scope.chain_is_pub && is_pub(&f.vis) => {
            facts
                .pub_items
                .insert((scope.file.to_string(), f.sig.ident.to_string()));
        }
        syn::Item::Mod(m) => walk_module(scope, m, facts),
        // A `pub use` inside a private module is collected too — whether it can
        // expose anything is decided later, from the declaring file's own
        // reachability.
        syn::Item::Use(u) if is_pub(&u.vis) => {
            collect_use(&u.tree, scope, &mut Vec::new(), facts);
        }
        _ => {}
    }
}

/// One `mod` item: an inline block becomes an addressable module key and
/// recurses; an out-of-line `mod x;` records a link whose visibility decides
/// whether the implementing file stays reachable.
/// Integration: shape dispatch + recursion.
fn walk_module(scope: Scope<'_>, m: &syn::ItemMod, facts: &mut FileFacts) {
    let mut nested = scope.mod_path.to_vec();
    nested.push(m.ident.to_string());
    let inner_scope = Scope {
        file: scope.file,
        mod_path: &nested,
        chain_is_pub: scope.chain_is_pub && is_pub(&m.vis),
    };
    match &m.content {
        Some((_, inner)) => {
            record_inline_module(scope.file, &nested, facts);
            walk_items(inner_scope, inner, facts);
        }
        None if inner_scope.chain_is_pub => {
            record_out_of_line_module(scope, m, facts);
        }
        None => {}
    }
}

/// Record a `pub mod child;` link plus, when the module carries a
/// `#[path = "…"]`, the file it really lives in — otherwise the link looks
/// for the conventional filename and a public module reads as unreachable.
/// Integration: link insert + optional alias.
fn record_out_of_line_module(scope: Scope<'_>, m: &syn::ItemMod, facts: &mut FileFacts) {
    facts.pub_mod_links.insert((
        scope.file.to_string(),
        scope.mod_path.to_vec(),
        m.ident.to_string(),
    ));
    if let Some(target) = path_attr_target(scope.file, &m.attrs) {
        let mut nested = scope.mod_path.to_vec();
        nested.push(m.ident.to_string());
        if let Some(key) = module_key(scope.file, &nested) {
            facts.inline_modules.push((key, target));
        }
    }
}

/// The file a `#[path = "…"]` attribute points at, resolved against the
/// declaring file's directory.
/// Operation: attribute extraction + path join.
fn path_attr_target(declaring_file: &str, attrs: &[syn::Attribute]) -> Option<String> {
    let value = attrs
        .iter()
        .find(|a| a.path().is_ident("path"))
        .and_then(|a| match &a.meta {
            syn::Meta::NameValue(nv) => Some(&nv.value),
            _ => None,
        })
        .and_then(|e| match e {
            syn::Expr::Lit(l) => match &l.lit {
                syn::Lit::Str(s) => Some(s.value()),
                _ => None,
            },
            _ => None,
        })?;
    let dir = declaring_file.replace('\\', "/");
    let dir = dir.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    Some(if dir.is_empty() {
        value
    } else {
        format!("{dir}/{value}")
    })
}

/// Note that `path` names an inline module living in `file`.
/// Operation: key build + push.
fn record_inline_module(file: &str, path: &[String], facts: &mut FileFacts) {
    if let Some(key) = module_key(file, path) {
        facts.inline_modules.push((key, file.to_string()));
    }
}

/// Record what a `pub use` tree exposes: named items (with their alias, if
/// any) and glob targets.
/// Integration: use-tree recursion accumulating the path prefix.
// qual:recursive
fn collect_use(
    tree: &syn::UseTree,
    scope: Scope<'_>,
    prefix: &mut Vec<String>,
    facts: &mut FileFacts,
) {
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            collect_use(&p.tree, scope, prefix, facts);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let name = n.ident.to_string();
            push_reexport(scope, prefix, &name, &name, facts);
        }
        // `use hidden::entry as public_entry` publishes `public_entry` while
        // the item is still `entry` in its own module — the chain needs both.
        syn::UseTree::Rename(r) => {
            push_reexport(
                scope,
                prefix,
                &r.rename.to_string(),
                &r.ident.to_string(),
                facts,
            );
        }
        syn::UseTree::Glob(_) => facts.globs.push(GlobUse {
            file: scope.file.to_string(),
            targets: target_keys(scope, prefix),
        }),
        syn::UseTree::Group(g) => g
            .items
            .iter()
            .for_each(|t| collect_use(t, scope, &mut prefix.clone(), facts)),
    }
}

/// Record one named re-export against every module its path might mean.
/// Operation: struct construction, key building delegated.
fn push_reexport(
    scope: Scope<'_>,
    prefix: &[String],
    exported: &str,
    source: &str,
    facts: &mut FileFacts,
) {
    facts.reexports.push(ReexportUse {
        file: scope.file.to_string(),
        targets: target_keys(scope, prefix),
        exported: exported.to_string(),
        source: source.to_string(),
    });
}

/// Every module key a `use` prefix might denote. `crate::`/`self::`/`super::`
/// are unambiguous; an unprefixed path can mean the current module's child or
/// a crate-root item (uniform paths), so both are offered and whichever
/// resolves wins — picking one would silently drop valid re-exports.
/// Operation: prefix normalisation, key building delegated.
fn target_keys(scope: Scope<'_>, prefix: &[String]) -> Vec<String> {
    candidate_paths(scope.mod_path, prefix)
        .iter()
        .filter_map(|p| module_key(scope.file, p))
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
