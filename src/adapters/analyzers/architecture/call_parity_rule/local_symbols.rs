//! Per-file local-symbol collection with mod-scope awareness.
//!
//! Two views are kept in sync:
//!
//! - `flat: HashSet<String>` — every name declared anywhere in the
//!   file, top-level or nested. Existing callers (`canonicalise_type
//!   _segments`, `local_symbols.contains(name)`) keep working unchanged.
//! - `by_name: HashMap<String, Vec<Vec<String>>>` — per-name list of
//!   mod-paths-within-file where the name is declared. Lets the
//!   scope-aware canonicaliser pick the closest enclosing declaration
//!   so `Session` referenced from inside `mod inner` resolves to
//!   `crate::<file>::inner::Session` when the type is declared there.

use crate::adapters::shared::cfg_test::has_cfg_test;
use crate::adapters::shared::use_tree::{AliasMap, ScopedAliasMap};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// `(flat-set, per-scope-map)` view over the names declared in a file.
#[derive(Debug, Default, Clone)]
pub(crate) struct LocalSymbols {
    pub flat: HashSet<String>,
    pub by_name: HashMap<String, Vec<Vec<String>>>,
}

/// All per-file lookup tables a resolver / call collector needs in
/// one place. Built once per file by the call-parity entry points;
/// borrowed into every `CanonScope` / `ResolveContext` / `FnContext`
/// / `InferContext` / `BuildContext` instead of duplicating the same
/// six fields across each context struct.
pub(crate) struct FileScope<'a> {
    pub path: &'a str,
    /// Top-level (file-scope) `use` aliases. Equivalent to
    /// `aliases_per_scope.get(&[])` when the scoped map was built via
    /// `gather_alias_map_scoped`; kept as a separate field so legacy /
    /// unit-test callers can populate just this one.
    pub alias_map: &'a AliasMap,
    /// Per-mod alias maps (output of `gather_alias_map_scoped`). Tests
    /// can pass an empty map; the lookup then falls back to
    /// `alias_map` for the legacy flat behaviour.
    pub aliases_per_scope: &'a ScopedAliasMap,
    pub local_symbols: &'a HashSet<String>,
    pub local_decl_scopes: &'a HashMap<String, Vec<Vec<String>>>,
    pub crate_root_modules: &'a HashSet<String>,
    /// All module paths in the workspace (multi-segment; derived from
    /// every file's `file_to_module_segments`). `None` for unit-test
    /// fixtures that don't construct a full workspace — disables the
    /// sibling-submodule discrimination in `normalize_after_alias`,
    /// preserving the legacy "external path returned as-is" behaviour.
    /// Populated by production setup (`build_workspace_files_map`)
    /// from the same `files` slice used to derive `crate_root_modules`.
    pub workspace_module_paths: Option<&'a HashSet<Vec<String>>>,
}

/// Inputs to `build_workspace_files_map`. Bundled because the per-file
/// pre-computed maps are themselves several arguments.
pub(crate) struct WorkspaceFilesInputs<'a> {
    pub files: &'a [(&'a str, &'a syn::File)],
    pub cfg_test_files: &'a HashSet<String>,
    pub aliases_per_file: &'a HashMap<String, AliasMap>,
    pub aliases_scoped_per_file: &'a HashMap<String, ScopedAliasMap>,
    pub local_symbols_per_file: &'a HashMap<String, LocalSymbols>,
    pub crate_root_modules: &'a HashSet<String>,
    /// Multi-segment module paths for every workspace file. Used by
    /// `normalize_after_alias` to discriminate sibling-submodule
    /// imports (`use response::X;`) from extern-crate imports
    /// (`use serde::Y;`) — same first-segment shape, different
    /// resolution semantics. `None` for unit-test fixtures with no
    /// real workspace.
    pub workspace_module_paths: Option<&'a HashSet<Vec<String>>>,
}

/// Pre-build a `FileScope` for every non-cfg-test workspace file.
/// Reused by the type-index build and the call-graph collector so each
/// file's lookup tables only get assembled once.
pub(crate) fn build_workspace_files_map<'a>(
    inputs: WorkspaceFilesInputs<'a>,
) -> HashMap<String, FileScope<'a>> {
    static EMPTY_SCOPED: OnceLock<ScopedAliasMap> = OnceLock::new();
    let empty_scoped: &'static ScopedAliasMap = EMPTY_SCOPED.get_or_init(ScopedAliasMap::new);
    let mut out = HashMap::new();
    for (path, _) in inputs.files {
        if inputs.cfg_test_files.contains(*path) {
            continue;
        }
        let Some(alias_map) = inputs.aliases_per_file.get(*path) else {
            continue;
        };
        let Some(local) = inputs.local_symbols_per_file.get(*path) else {
            continue;
        };
        let aliases_per_scope = inputs
            .aliases_scoped_per_file
            .get(*path)
            .unwrap_or(empty_scoped);
        out.insert(
            path.to_string(),
            FileScope {
                path,
                alias_map,
                aliases_per_scope,
                local_symbols: &local.flat,
                local_decl_scopes: &local.by_name,
                crate_root_modules: inputs.crate_root_modules,
                workspace_module_paths: inputs.workspace_module_paths,
            },
        );
    }
    out
}

/// Build the multi-segment module-path set from the workspace.
///
/// This answers the **module membership** question: "is `[parent…,
/// foo]` a real submodule of `parent` in the workspace?" — needed
/// by `normalize_after_alias` to discriminate `use foo::T` between a
/// sibling submodule and an extern crate. Membership is independent
/// of public visibility: a private `mod foo;` (no `pub`) still makes
/// `foo` a child of the enclosing module for code inside the parent.
///
/// Using `file_root_visibility` (which checks `pub` at every non-root
/// link) here would be too narrow — it filters by EXTERNAL reach, not
/// declaration. A private `mod` link upstream would drop legitimate
/// child paths even though their importers (which live inside the
/// hidden subtree) can name them.
///
/// We walk from crate roots through every declared `mod X;` and
/// `mod X { … }` (regardless of visibility), skipping cfg-test mods.
/// File-backed children are descended into via the workspace
/// segs-to-path map; inline children are descended into directly.
/// Stale files that no `mod` declaration reaches stay excluded — that
/// part of the `file_root_visibility`-based filter (the
/// orphan-exclusion property exercised by bug 9) is preserved.
pub(crate) fn collect_workspace_module_paths(files: &[(&str, &syn::File)]) -> HashSet<Vec<String>> {
    let segs_to_path =
        crate::adapters::analyzers::architecture::forbidden_rule::build_module_segs_to_path_map(
            files,
        );
    let path_to_ast: HashMap<&str, &syn::File> = files.iter().map(|(p, ast)| (*p, *ast)).collect();
    let ctx = ModuleWalkCtx {
        segs_to_path: &segs_to_path,
        path_to_ast: &path_to_ast,
    };
    let crate_root_paths: Vec<&str> = files
        .iter()
        .filter(|(p, _)| matches!(*p, "src/lib.rs" | "src/main.rs"))
        .map(|(p, _)| *p)
        .collect();
    let mut out = HashSet::new();
    if crate_root_paths.is_empty() {
        walk_fallback_roots(files, &segs_to_path, &ctx, &mut out);
    } else {
        for root_path in &crate_root_paths {
            let ast = path_to_ast[root_path];
            out.insert(Vec::new());
            walk_declared_module_paths(&ast.items, &[], &ctx, &mut out);
        }
    }
    out
}

/// Workspace-wide lookups consumed by the declared-module-paths walker.
/// Keeps the recursive helper from juggling two `HashMap` parameters.
struct ModuleWalkCtx<'a> {
    segs_to_path: &'a HashMap<Vec<String>, &'a str>,
    path_to_ast: &'a HashMap<&'a str, &'a syn::File>,
}

/// Walk every `mod X` declaration reachable from `items` at module
/// `stack`. Inserts the full module path for each non-cfg-test
/// declaration and descends into both inline content and file-backed
/// child files. Operation: closure-hidden recursion.
// qual:recursive
fn walk_declared_module_paths(
    items: &[syn::Item],
    stack: &[String],
    ctx: &ModuleWalkCtx<'_>,
    out: &mut HashSet<Vec<String>>,
) {
    let recurse = |inner: &[syn::Item], next: &[String], out: &mut HashSet<Vec<String>>| {
        walk_declared_module_paths(inner, next, ctx, out);
    };
    for item in items {
        let syn::Item::Mod(m) = item else {
            continue;
        };
        if has_cfg_test(&m.attrs) {
            continue;
        }
        let mut next = stack.to_vec();
        next.push(m.ident.to_string());
        out.insert(next.clone());
        if let Some((_, inner)) = m.content.as_ref() {
            recurse(inner, &next, out);
        } else if let Some(child_ast) = file_backed_child_ast(&next, ctx) {
            recurse(child_ast, &next, out);
        }
    }
}

/// Resolve a file-backed `mod foo;` declaration (`m.content.is_none()`)
/// to the items of the workspace file that backs it, if any. `None`
/// means no workspace file matches the expected segs path — either the
/// declaration is orphaned or the file is renamed via `#[path]` (an
/// existing limitation, see file_to_module_segments). Operation.
fn file_backed_child_ast<'a>(next: &[String], ctx: &ModuleWalkCtx<'a>) -> Option<&'a [syn::Item]> {
    let child_path = ctx.segs_to_path.get(next)?;
    ctx.path_to_ast
        .get(child_path)
        .map(|ast| ast.items.as_slice())
}

/// Fallback for unit-test fixtures with no `src/lib.rs` / `src/main.rs`:
/// every top-level file (= no ancestor file present in the workspace)
/// is treated as its own implicit root. Orphan files whose parent IS
/// in the workspace but whose ancestor never declares `mod <them>` are
/// reached only through that ancestor's walker, so missing declarations
/// keep them out of `out` (bug 9 invariant).
///
/// Tie-break: when both `src/foo.rs` and `src/foo/mod.rs` are in the
/// workspace (stale-leftover), they collide on `["foo"]` and
/// `build_module_segs_to_path_map` already picked one as the winner.
/// The walker MUST honour that — iterating the raw `files` slice would
/// let the loser register its own (stale) submodule declarations and
/// re-introduce the false workspace edges the tie-break was meant to
/// rule out. So we skip any file whose `segs_to_path[&base]` doesn't
/// point at our own path. Operation.
fn walk_fallback_roots(
    files: &[(&str, &syn::File)],
    segs_to_path: &HashMap<Vec<String>, &str>,
    ctx: &ModuleWalkCtx<'_>,
    out: &mut HashSet<Vec<String>>,
) {
    for (path, ast) in files {
        let base =
            crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments(path);
        if has_workspace_ancestor(&base, segs_to_path) {
            continue;
        }
        if !crate::adapters::analyzers::architecture::forbidden_rule::is_tie_break_winner(
            path,
            &base,
            segs_to_path,
        ) {
            continue;
        }
        out.insert(base.clone());
        walk_declared_module_paths(&ast.items, &base, ctx, out);
    }
}

/// True when some strict prefix of `base` is itself a file path in the
/// workspace — i.e. `base` has an ancestor that will reach it (or
/// pointedly NOT reach it) via its own walker. Operation: prefix scan.
fn has_workspace_ancestor(base: &[String], segs_to_path: &HashMap<Vec<String>, &str>) -> bool {
    // `Vec<T>: Borrow<[T]>` — pass the slice directly so the lookup
    // doesn't allocate a fresh `Vec` per prefix probe.
    (1..base.len()).any(|len| segs_to_path.contains_key(&base[..len] as &[String]))
}

/// Bundles the workspace-derived metadata that every call-parity
/// pre-pass needs: which files are cfg-test (skip them), which
/// crate-root module names exist (Rust 2018+ absolute imports),
/// and which multi-segment module paths exist (sibling-submodule
/// import discrimination). All three are derived from the same
/// `files` slice once at the call_parity entry point and threaded
/// through; bundling avoids each helper carrying three separate
/// `&HashSet<...>` parameters.
pub(crate) struct WorkspaceLookup<'a> {
    pub cfg_test_files: &'a HashSet<String>,
    pub crate_root_modules: &'a HashSet<String>,
    pub workspace_module_paths: &'a HashSet<Vec<String>>,
}

/// Top-level-only name set for callers that don't track mod scope.
/// Names declared exclusively inside nested inline `mod`s are
/// reachable through `collect_local_symbols_scoped` only — exposing
/// them flat would let the legacy resolution path (which falls back
/// to "treat any hit as top-level" when `local_decl_scopes` is empty)
/// produce bogus `crate::<file>::Inner` paths for inner-module-only
/// names. Operation: project the names with at least one top-level
/// declaration scope.
pub(crate) fn collect_local_symbols(ast: &syn::File) -> HashSet<String> {
    let scoped = collect_local_symbols_scoped(ast);
    scoped
        .by_name
        .into_iter()
        .filter_map(|(name, scopes)| scopes.iter().any(|p| p.is_empty()).then_some(name))
        .collect()
}

/// Scoped variant. Returns both views in one walk so the `flat` set
/// and the `by_name` map are always consistent. Operation.
pub(crate) fn collect_local_symbols_scoped(ast: &syn::File) -> LocalSymbols {
    let mut symbols = LocalSymbols::default();
    walk_local_symbols(&ast.items, &mut Vec::new(), &mut symbols);
    symbols
}

/// Recursive AST walk that populates `LocalSymbols.flat` + `by_name`.
/// `mod_stack` carries the current mod-scope (outer-most first).
/// Operation. Own calls hidden in closure for IOSP leniency.
// qual:recursive
fn walk_local_symbols(items: &[syn::Item], mod_stack: &mut Vec<String>, out: &mut LocalSymbols) {
    let recurse = |inner: &[syn::Item], stack: &mut Vec<String>, out: &mut LocalSymbols| {
        walk_local_symbols(inner, stack, out);
    };
    for item in items {
        if let Some(name) = item_name(item) {
            out.flat.insert(name.clone());
            out.by_name.entry(name).or_default().push(mod_stack.clone());
        }
        if let syn::Item::Mod(m) = item {
            if !has_cfg_test(&m.attrs) {
                if let Some((_, inner)) = m.content.as_ref() {
                    mod_stack.push(m.ident.to_string());
                    recurse(inner, mod_stack, out);
                    mod_stack.pop();
                }
            }
        }
    }
}

/// Look up the mod-path in which `name` is declared at exactly the
/// current `mod_stack` scope. Rust resolves unqualified names against
/// the current module only — child modules don't inherit parent
/// declarations — so this intentionally does *not* walk outward.
///
/// An empty `decl_scopes` map means "scope tracking not populated"
/// (test fixtures without `collect_local_symbols_scoped`); the
/// canonicaliser then falls back to flat top-level prepend. A
/// populated map with no exact match returns `None` so the caller
/// skips the same-file branch entirely.
pub(crate) fn scope_for_local<'a>(
    decl_scopes: &'a HashMap<String, Vec<Vec<String>>>,
    name: &str,
    mod_stack: &[String],
) -> Option<&'a [String]> {
    if decl_scopes.is_empty() {
        return Some(&[]);
    }
    let candidates = decl_scopes.get(name)?;
    candidates
        .iter()
        .find(|path| path.as_slice() == mod_stack)
        .map(Vec::as_slice)
}

/// Extract the declared ident from an `Item` if it has one
/// `local_symbols` cares about. Operation: lookup table.
fn item_name(item: &syn::Item) -> Option<String> {
    match item {
        syn::Item::Fn(f) => Some(f.sig.ident.to_string()),
        syn::Item::Mod(m) => Some(m.ident.to_string()),
        syn::Item::Struct(s) => Some(s.ident.to_string()),
        syn::Item::Enum(e) => Some(e.ident.to_string()),
        syn::Item::Union(u) => Some(u.ident.to_string()),
        syn::Item::Trait(t) => Some(t.ident.to_string()),
        syn::Item::Type(t) => Some(t.ident.to_string()),
        syn::Item::Const(c) => Some(c.ident.to_string()),
        syn::Item::Static(s) => Some(s.ident.to_string()),
        _ => None,
    }
}
