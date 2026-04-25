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
use crate::adapters::shared::use_tree::ScopedAliasMap;
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
    pub alias_map: &'a HashMap<String, Vec<String>>,
    /// Per-mod alias maps (output of `gather_alias_map_scoped`). Tests
    /// can pass an empty map; the lookup then falls back to
    /// `alias_map` for the legacy flat behaviour.
    pub aliases_per_scope: &'a ScopedAliasMap,
    pub local_symbols: &'a HashSet<String>,
    pub local_decl_scopes: &'a HashMap<String, Vec<Vec<String>>>,
    pub crate_root_modules: &'a HashSet<String>,
}

/// Inputs to `build_workspace_files_map`. Bundled because the per-file
/// pre-computed maps are themselves several arguments.
pub(crate) struct WorkspaceFilesInputs<'a> {
    pub files: &'a [(&'a str, &'a syn::File)],
    pub cfg_test_files: &'a HashSet<String>,
    pub aliases_per_file: &'a HashMap<String, HashMap<String, Vec<String>>>,
    pub aliases_scoped_per_file: &'a HashMap<String, ScopedAliasMap>,
    pub local_symbols_per_file: &'a HashMap<String, LocalSymbols>,
    pub crate_root_modules: &'a HashSet<String>,
}

// qual:api
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
            },
        );
    }
    out
}

// qual:api
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

// qual:api
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

// qual:api
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
