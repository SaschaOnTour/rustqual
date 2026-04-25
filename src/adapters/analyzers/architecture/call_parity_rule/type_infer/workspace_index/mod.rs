//! `WorkspaceTypeIndex` — lookup tables the inference engine queries.
//!
//! Three maps are populated in one walk over every non-cfg-test file:
//!
//! - `struct_fields`: `(struct_canonical, field_name)` → field type
//! - `method_returns`: `(receiver_canonical, method_name)` → return type
//! - `fn_returns`: `canonical_free_fn_name` → return type
//!
//! Each sub-module owns one collector. They share a `BuildContext` with
//! the per-file resolution inputs; collectors don't talk to each other.

pub mod aliases;
pub mod fields;
pub mod functions;
pub mod methods;
pub mod traits;

use super::canonical::CanonicalType;
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
    collect_local_symbols_scoped, LocalSymbols,
};
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use crate::adapters::shared::use_tree::ScopedAliasMap;
use std::collections::{HashMap, HashSet};

/// Per-file resolution context passed to every collector. Owned by the
/// outer build loop, borrowed into each `collect_from_file` call.
pub(super) struct BuildContext<'a> {
    pub path: &'a str,
    pub alias_map: &'a HashMap<String, Vec<String>>,
    pub aliases_per_scope: &'a ScopedAliasMap,
    pub local_symbols: &'a HashSet<String>,
    /// Per-name list of mod-paths-within-file where `local_symbols`
    /// names are declared. Lets the resolver pick
    /// `crate::<file>::<mod>::Session` over `crate::<file>::Session`
    /// when `Session` is declared inside an inline mod.
    pub local_decl_scopes: &'a HashMap<String, Vec<Vec<String>>>,
    pub crate_root_modules: &'a HashSet<String>,
    /// user-wrapper names peeled during resolution. Shared
    /// across the whole build.
    pub transparent_wrappers: &'a HashSet<String>,
    /// type aliases already collected across the workspace.
    /// `None` in pass 1 (the alias collector itself); `Some(&…)` in
    /// pass 2 so fields/methods/functions/traits that reference an
    /// alias are resolved through the alias target instead of caching
    /// the raw alias name.
    pub type_aliases: Option<&'a HashMap<String, (Vec<String>, syn::Type)>>,
}

/// Build a canonical type-path key by prefixing the impl/trait segments
/// with `crate::<file-module>::<inline-mods>::` unless they're already
/// crate-rooted. `mod_stack` carries the names of enclosing inline
/// `mod inner { ... }` blocks so items declared inside them key as
/// `crate::<file>::inner::X`, matching the path a call-site like
/// `inner::X` canonicalises to.
/// Operation.
pub(super) fn canonical_type_key(
    segs: &[String],
    ctx: &BuildContext<'_>,
    mod_stack: &[String],
) -> String {
    if segs.first().map(String::as_str) == Some("crate") {
        return segs.join("::");
    }
    let mut out: Vec<String> = vec!["crate".to_string()];
    out.extend(file_to_module_segments(ctx.path));
    out.extend(mod_stack.iter().cloned());
    out.extend(segs.iter().cloned());
    out.join("::")
}

/// Build a `ResolveContext` from the shared `BuildContext` inputs —
/// extracted so the per-field / per-method / per-free-fn collectors
/// don't each repeat the same construction. `type_aliases` propagates
/// through so pass-2 collectors (running after the alias-collector
/// populated them) resolve aliased types transparently. `mod_stack` is
/// the current mod-path inside `ctx.path` — pass `&[]` for top-level
/// items.
/// Operation.
pub(super) fn resolve_ctx_from_build<'a>(
    ctx: &'a BuildContext<'a>,
    mod_stack: &'a [String],
) -> super::resolve::ResolveContext<'a> {
    super::resolve::ResolveContext {
        alias_map: ctx.alias_map,
        local_symbols: ctx.local_symbols,
        crate_root_modules: ctx.crate_root_modules,
        importing_file: ctx.path,
        type_aliases: ctx.type_aliases,
        transparent_wrappers: Some(ctx.transparent_wrappers),
        local_decl_scopes: Some(ctx.local_decl_scopes),
        aliases_per_scope: Some(ctx.aliases_per_scope),
        mod_stack,
    }
}

/// Lookup tables populated from one walk over the workspace.
#[derive(Default)]
pub struct WorkspaceTypeIndex {
    /// `(struct_canonical, field_name) → canonical field type`.
    pub struct_fields: HashMap<(String, String), CanonicalType>,
    /// `(receiver_type_canonical, method_name) → canonical return type`.
    pub method_returns: HashMap<(String, String), CanonicalType>,
    /// `canonical_free_fn_name → canonical return type`.
    pub fn_returns: HashMap<String, CanonicalType>,
    /// `trait_canonical → [impl_type_canonical, …]`. Every
    /// `impl Trait for X` in the workspace contributes one entry so
    /// trait-dispatch can over-approximate edges to every impl.
    pub trait_impls: HashMap<String, Vec<String>>,
    /// `trait_canonical → {method_name, …}`. Gates
    /// trait-dispatch so `dyn Trait.unrelated_method()` stays
    /// unresolved.
    pub trait_methods: HashMap<String, std::collections::HashSet<String>>,
    /// `alias_canonical → (generic_param_names, target)`.
    /// Params are captured so use-sites like `Alias<ArgA>` can
    /// substitute the params' idents in `target` before resolution.
    /// Aliases without generics just have an empty `Vec`.
    pub type_aliases: HashMap<String, (Vec<String>, syn::Type)>,
    /// user-configured last-ident names to treat as
    /// transparent single-type-param wrappers (framework extractors
    /// like `State<T>` / `Data<T>`). Mirrored from the
    /// `CompiledCallParity.transparent_wrappers` at build time.
    pub transparent_wrappers: HashSet<String>,
}

impl WorkspaceTypeIndex {
    pub fn new() -> Self {
        Self::default()
    }

    // qual:api
    /// Look up a struct field's canonical type. Operation.
    pub fn struct_field(&self, type_canonical: &str, field: &str) -> Option<&CanonicalType> {
        self.struct_fields
            .get(&(type_canonical.to_string(), field.to_string()))
    }

    // qual:api
    /// Look up a method's return type. Operation.
    pub fn method_return(&self, receiver_canonical: &str, method: &str) -> Option<&CanonicalType> {
        self.method_returns
            .get(&(receiver_canonical.to_string(), method.to_string()))
    }

    // qual:api
    /// Look up a free-fn's return type. Operation.
    pub fn fn_return(&self, fn_canonical: &str) -> Option<&CanonicalType> {
        self.fn_returns.get(fn_canonical)
    }

    // qual:api
    /// Look up all workspace impls of a trait. Returns an empty slice
    /// when the trait has no impls recorded. Operation.
    pub fn impls_of_trait(&self, trait_canonical: &str) -> &[String] {
        self.trait_impls
            .get(trait_canonical)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    // qual:api
    /// True iff `method_name` is declared on `trait_canonical`.
    /// Operation.
    pub fn trait_has_method(&self, trait_canonical: &str, method_name: &str) -> bool {
        self.trait_methods
            .get(trait_canonical)
            .is_some_and(|methods| methods.contains(method_name))
    }
}

// qual:api
/// Bundled input for `build_workspace_type_index`. Bundles per-file
/// pre-computed maps + the workspace-wide flag set so the entry-point
/// signature stays under the SRP param count.
pub struct WorkspaceIndexInputs<'a> {
    pub files: &'a [(&'a str, &'a syn::File)],
    pub aliases_per_file: &'a HashMap<String, HashMap<String, Vec<String>>>,
    pub aliases_scoped_per_file: &'a HashMap<String, ScopedAliasMap>,
    pub local_symbols_per_file: &'a HashMap<String, LocalSymbols>,
    pub cfg_test_files: &'a HashSet<String>,
    pub crate_root_modules: &'a HashSet<String>,
    pub transparent_wrappers: &'a HashSet<String>,
}

// qual:api
/// Build the workspace type index from parsed files + their pre-computed
/// alias maps and `LocalSymbols`. Skips cfg-test files wholesale.
/// `transparent_wrappers` seeds the user-configured wrapper list onto
/// the index so downstream inference peels them just like `Arc` / `Box`.
///
/// Runs in two passes: first collects type aliases across every file,
/// then collects fields/methods/functions/traits with the alias map
/// populated so aliased return types (`fn foo() -> AppResult<T>`)
/// resolve through to their targets instead of caching the raw alias
/// path. Integration.
pub fn build_workspace_type_index(inputs: &WorkspaceIndexInputs<'_>) -> WorkspaceTypeIndex {
    let mut index = WorkspaceTypeIndex::new();
    index.transparent_wrappers = inputs.transparent_wrappers.clone();
    let shared = |type_aliases| WalkInputs {
        files: inputs.files,
        aliases_per_file: inputs.aliases_per_file,
        aliases_scoped_per_file: inputs.aliases_scoped_per_file,
        local_symbols_per_file: inputs.local_symbols_per_file,
        cfg_test_files: inputs.cfg_test_files,
        crate_root_modules: inputs.crate_root_modules,
        transparent_wrappers: inputs.transparent_wrappers,
        type_aliases,
    };
    // Pass 1: aliases across all files (no alias map yet).
    walk_files(&shared(None), &mut index, |index, ctx, ast| {
        aliases::collect_from_file(index, ctx, ast)
    });
    // Pass 2: fields/methods/functions/traits with alias map visible.
    // `mem::take` lets us borrow the alias map immutably while still
    // mutating other fields of `index`; we restore at the end.
    let collected_aliases = std::mem::take(&mut index.type_aliases);
    walk_files(
        &shared(Some(&collected_aliases)),
        &mut index,
        |index, ctx, ast| {
            fields::collect_from_file(index, ctx, ast);
            methods::collect_from_file(index, ctx, ast);
            functions::collect_from_file(index, ctx, ast);
            traits::collect_from_file(index, ctx, ast);
        },
    );
    index.type_aliases = collected_aliases;
    index
}

/// Inputs common to both index-build passes. Bundled so `walk_files`
/// doesn't exceed the SRP parameter count.
struct WalkInputs<'a> {
    files: &'a [(&'a str, &'a syn::File)],
    aliases_per_file: &'a HashMap<String, HashMap<String, Vec<String>>>,
    aliases_scoped_per_file: &'a HashMap<String, ScopedAliasMap>,
    local_symbols_per_file: &'a HashMap<String, LocalSymbols>,
    cfg_test_files: &'a HashSet<String>,
    crate_root_modules: &'a HashSet<String>,
    transparent_wrappers: &'a HashSet<String>,
    type_aliases: Option<&'a HashMap<String, (Vec<String>, syn::Type)>>,
}

/// Shared file-walk scaffold for both index build passes. Skips
/// cfg-test files and files without a pre-computed alias map; hands
/// the per-file `BuildContext` to `visit`. Integration.
fn walk_files<F>(inputs: &WalkInputs<'_>, index: &mut WorkspaceTypeIndex, mut visit: F)
where
    F: FnMut(&mut WorkspaceTypeIndex, &BuildContext<'_>, &syn::File),
{
    for (path, ast) in inputs.files {
        if inputs.cfg_test_files.contains(*path) {
            continue;
        }
        let Some(alias_map) = inputs.aliases_per_file.get(*path) else {
            continue;
        };
        let Some(local) = inputs.local_symbols_per_file.get(*path) else {
            continue;
        };
        let empty_scoped = HashMap::new();
        let aliases_per_scope = inputs
            .aliases_scoped_per_file
            .get(*path)
            .unwrap_or(&empty_scoped);
        let ctx = BuildContext {
            path,
            alias_map,
            aliases_per_scope,
            local_symbols: &local.flat,
            local_decl_scopes: &local.by_name,
            crate_root_modules: inputs.crate_root_modules,
            transparent_wrappers: inputs.transparent_wrappers,
            type_aliases: inputs.type_aliases,
        };
        visit(index, &ctx, ast);
    }
}
