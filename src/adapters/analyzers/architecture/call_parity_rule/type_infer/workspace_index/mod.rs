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
use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
use crate::adapters::analyzers::architecture::forbidden_rule::file_to_module_segments;
use std::collections::{HashMap, HashSet};

/// Per-file resolution context passed to every collector. Owned by the
/// outer build loop, borrowed into each `collect_from_file` call.
pub(super) struct BuildContext<'a> {
    pub file: &'a FileScope<'a>,
    /// All workspace `FileScope`s, keyed by file path. Lets the
    /// resolver switch into an alias's declaring scope when expanding
    /// imported aliases.
    pub workspace_files: &'a HashMap<String, FileScope<'a>>,
    /// User-wrapper names peeled during resolution. Shared across the
    /// whole build.
    pub transparent_wrappers: &'a HashSet<String>,
    /// Type aliases already collected across the workspace. `None` in
    /// pass 1 (the alias collector itself); `Some(&…)` in pass 2.
    pub type_aliases: Option<&'a HashMap<String, AliasDef>>,
}

/// Source location of a recorded type-index entry (currently used for
/// trait-method declarations). 1-based line, 0-based column.
#[derive(Debug, Clone)]
pub struct MethodLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

/// Workspace type-alias entry, keyed under the alias's canonical name.
/// `target` is resolved against the alias's *own* declaring scope, not
/// the use-site's, so cross-module aliases (`use crate::store::Store;
/// type Repo = Arc<Store>;`) expand correctly.
pub struct AliasDef {
    /// Generic parameter names (`["T"]` for `type AppResult<T> = …`).
    pub params: Vec<String>,
    /// Right-hand side of the alias.
    pub target: syn::Type,
    /// Path of the file where the alias was declared.
    pub decl_file: String,
    /// Mod-stack inside `decl_file` of the alias's enclosing module.
    pub decl_mod_stack: Vec<String>,
}

/// Build a canonical type-path key by prefixing the impl/trait segments
/// with `crate::<file-module>::<inline-mods>::` unless they're already
/// crate-rooted. `mod_stack` carries the names of enclosing inline
/// `mod inner { ... }` blocks so items declared inside them key as
/// `crate::<file>::inner::X`, matching the path a call-site like
/// `inner::X` canonicalises to.
pub(super) fn canonical_type_key(
    segs: &[String],
    ctx: &BuildContext<'_>,
    mod_stack: &[String],
) -> String {
    if segs.first().map(String::as_str) == Some("crate") {
        return segs.join("::");
    }
    let mut out: Vec<String> = vec!["crate".to_string()];
    out.extend(file_to_module_segments(ctx.file.path));
    out.extend(mod_stack.iter().cloned());
    out.extend(segs.iter().cloned());
    out.join("::")
}

/// Build a `ResolveContext` from the shared `BuildContext` inputs —
/// extracted so the per-field / per-method / per-free-fn collectors
/// don't each repeat the same construction. `mod_stack` is the current
/// mod-path inside `ctx.file.path` — pass `&[]` for top-level items.
pub(super) fn resolve_ctx_from_build<'a>(
    ctx: &'a BuildContext<'a>,
    mod_stack: &'a [String],
) -> super::resolve::ResolveContext<'a> {
    super::resolve::ResolveContext {
        file: ctx.file,
        mod_stack,
        type_aliases: ctx.type_aliases,
        transparent_wrappers: Some(ctx.transparent_wrappers),
        workspace_files: Some(ctx.workspace_files),
        alias_param_subs: None,
    }
}

/// Lookup tables populated from one walk over the workspace.
///
/// `struct_fields` and `method_returns` use a nested map shape
/// (outer keyed by canonical type, inner by field/method) so the hot
/// `infer_field` / `infer_method_call` paths can probe with `&str`s
/// against the inner map without allocating a `(String, String)` key
/// per lookup. The dedicated `insert_struct_field` /
/// `insert_method_return` helpers keep call-sites tidy in production
/// and tests.
#[derive(Default)]
pub struct WorkspaceTypeIndex {
    /// `struct_canonical → {field_name → canonical field type}`.
    pub struct_fields: HashMap<String, HashMap<String, CanonicalType>>,
    /// `receiver_canonical → {method_name → canonical return type}`.
    pub method_returns: HashMap<String, HashMap<String, CanonicalType>>,
    /// `canonical_free_fn_name → canonical return type`.
    pub fn_returns: HashMap<String, CanonicalType>,
    /// `trait_canonical → [impl_type_canonical, …]`. Every
    /// `impl Trait for X` in the workspace contributes one entry so
    /// the anchor index can populate `AnchorInfo.impl_layers` and
    /// `AnchorInfo.impl_method_canonicals` (the unified target-
    /// capability rule reads both).
    pub trait_impls: HashMap<String, Vec<String>>,
    /// `trait_canonical → {method_name, …}`. Gates
    /// trait-dispatch so `dyn Trait.unrelated_method()` stays
    /// unresolved.
    pub trait_methods: HashMap<String, std::collections::HashSet<String>>,
    /// `trait_canonical → bool` carrying the trait's `pub` visibility
    /// modifier. A `false` entry means the trait was declared without
    /// `pub` (`trait Internal { … }`) and isn't part of the public
    /// architectural surface — the unified target-capability rule
    /// rejects such anchors so they don't trigger Check B/D findings
    /// for implementation-detail traits. Missing entries default to
    /// `false` (defensive — synthetic test fixtures or future
    /// builders without visibility capture stay invisible).
    pub trait_visibility: HashMap<String, bool>,
    /// `(trait_canonical, method_name) → source location` (file +
    /// 1-based line + column). Carried alongside `trait_methods` so
    /// synthetic anchor findings can attach a real source location
    /// (suppression windows, orphan detector, SARIF).
    pub trait_method_locations: HashMap<(String, String), MethodLocation>,
    /// `(trait_canonical, method_name)` for every trait method that
    /// declares a default body (`fn m(&self) { … }` inside the trait
    /// itself, not just a signature). Drives the unified target-
    /// capability rule: a trait declared in the target layer is a
    /// capability iff the method either has a default body OR an
    /// overriding impl in the target — methods that are pure
    /// signatures (no default, no impl) are uncallable and not a
    /// capability.
    pub trait_methods_with_default_body: HashSet<(String, String)>,
    /// `trait_canonical → {impl_type_canonical → {overridden_method, …}}`.
    /// For every `impl Trait for X { … }`, records which methods the
    /// impl block actually defines. Default-method dispatch routes
    /// to `<trait>::<method>` when the impl doesn't override —
    /// otherwise to `<impl>::<method>`. Without this, dispatch
    /// would fabricate an `impl::method` graph node that doesn't
    /// exist (the body lives on the trait).
    pub trait_impl_overrides: HashMap<String, HashMap<String, std::collections::HashSet<String>>>,
    /// `alias_canonical → AliasDef`. Use-sites substitute generic args
    /// into `target` and resolve the result against the alias's own
    /// `decl_file` / `decl_mod_stack` scope (not the use-site's).
    pub type_aliases: HashMap<String, AliasDef>,
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
    /// Look up a struct field's canonical type. Two `&str` probes
    /// against the nested map — no allocation. Operation.
    pub fn struct_field(&self, type_canonical: &str, field: &str) -> Option<&CanonicalType> {
        self.struct_fields.get(type_canonical)?.get(field)
    }

    // qual:api
    /// Look up a method's return type. Two `&str` probes against the
    /// nested map — no allocation. Operation.
    pub fn method_return(&self, receiver_canonical: &str, method: &str) -> Option<&CanonicalType> {
        self.method_returns.get(receiver_canonical)?.get(method)
    }

    // qual:api
    /// Insert a `(type, field) → canonical` entry. Builds the nested
    /// map shape on demand. Operation.
    pub fn insert_struct_field(
        &mut self,
        type_canonical: impl Into<String>,
        field: impl Into<String>,
        ty: CanonicalType,
    ) {
        self.struct_fields
            .entry(type_canonical.into())
            .or_default()
            .insert(field.into(), ty);
    }

    // qual:api
    /// Insert a `(receiver, method) → canonical` entry. Builds the
    /// nested map shape on demand. Operation.
    pub fn insert_method_return(
        &mut self,
        receiver_canonical: impl Into<String>,
        method: impl Into<String>,
        ret: CanonicalType,
    ) {
        self.method_returns
            .entry(receiver_canonical.into())
            .or_default()
            .insert(method.into(), ret);
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

    // qual:api
    /// Iterate every workspace trait and its declared methods. Used by
    /// the call-graph builder to mirror trait+method pairs into anchor
    /// index. Operation.
    pub fn trait_methods_iter(&self) -> impl Iterator<Item = (&String, &HashSet<String>)> {
        self.trait_methods.iter()
    }

    // qual:api
    /// True iff the trait method declares a default body in the trait
    /// itself. Used by the unified anchor-as-target-capability rule to
    /// distinguish callable defaults from pure signatures. Operation.
    pub fn trait_method_has_default_body(&self, trait_canonical: &str, method_name: &str) -> bool {
        self.trait_methods_with_default_body
            .contains(&(trait_canonical.to_string(), method_name.to_string()))
    }

    // qual:api
    /// True iff the trait was declared with a `pub` visibility modifier
    /// (and thus is part of the public architectural surface). Returns
    /// `false` for traits without `pub` and for traits not recorded in
    /// the index — defensive default so synthetic fixtures or future
    /// build paths without visibility capture treat the trait as
    /// invisible. The unified target-capability rule consults this so
    /// private implementation-detail traits don't surface as Check B/D
    /// targets. Operation.
    pub fn trait_is_visible(&self, trait_canonical: &str) -> bool {
        self.trait_visibility
            .get(trait_canonical)
            .copied()
            .unwrap_or(false)
    }

    // qual:api
    /// Look up the source location of a trait-method declaration.
    /// Returns `None` for traits / methods not recorded in the index
    /// (e.g. test-only items, synthetic test fixtures). Used by the
    /// call-graph builder to attach a real location to synthetic
    /// trait-method anchors. Operation.
    pub fn trait_method_location(
        &self,
        trait_canonical: &str,
        method_name: &str,
    ) -> Option<&MethodLocation> {
        self.trait_method_locations
            .get(&(trait_canonical.to_string(), method_name.to_string()))
    }

    // qual:api
    /// Set of impl-self-type canonicals that override `method_name` on
    /// `trait_canonical`. Used by the call-graph builder to populate the
    /// anchor → impls map (`CallGraph::trait_method_anchors`). Operation.
    pub fn overriding_impls_for(
        &self,
        trait_canonical: &str,
        method_name: &str,
    ) -> HashSet<String> {
        self.impls_of_trait(trait_canonical)
            .iter()
            .filter(|impl_type| self.impl_overrides_method(trait_canonical, impl_type, method_name))
            .cloned()
            .collect()
    }

    // qual:api
    /// True iff `impl_type_canonical` overrides `method_name` in its
    /// `impl trait_canonical for impl_type_canonical { … }` block.
    /// Returns false when the impl inherits the trait's default body
    /// for that method. Returns **true** when there's no record at
    /// all — preserves the "assume override" behaviour for
    /// hand-built test indices that populate `trait_impls` without
    /// `trait_impl_overrides`. The production builder
    /// (`traits.rs::record_trait_impl`) always populates both. Used by
    /// `overriding_impls_for` to drive the `<Trait>::<method>` anchor
    /// → impl-layers map; non-overriding impls don't contribute layers
    /// to the anchor (so the walker won't recognise the anchor as a
    /// target boundary purely on default-body impls). Operation.
    pub fn impl_overrides_method(
        &self,
        trait_canonical: &str,
        impl_type_canonical: &str,
        method_name: &str,
    ) -> bool {
        match self
            .trait_impl_overrides
            .get(trait_canonical)
            .and_then(|by_impl| by_impl.get(impl_type_canonical))
        {
            Some(methods) => methods.contains(method_name),
            None => true,
        }
    }
}

// qual:api
/// Bundled input for `build_workspace_type_index`. Bundles per-file
/// pre-computed maps + the workspace-wide flag set so the entry-point
/// signature stays under the SRP param count.
pub struct WorkspaceIndexInputs<'a> {
    pub files: &'a [(&'a str, &'a syn::File)],
    pub workspace_files: &'a HashMap<String, FileScope<'a>>,
    pub cfg_test_files: &'a HashSet<String>,
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
        cfg_test_files: inputs.cfg_test_files,
        files: inputs.files,
        transparent_wrappers: inputs.transparent_wrappers,
        workspace_files: inputs.workspace_files,
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

/// Inputs common to both index-build passes.
struct WalkInputs<'a> {
    files: &'a [(&'a str, &'a syn::File)],
    workspace_files: &'a HashMap<String, FileScope<'a>>,
    cfg_test_files: &'a HashSet<String>,
    transparent_wrappers: &'a HashSet<String>,
    type_aliases: Option<&'a HashMap<String, AliasDef>>,
}

/// Shared file-walk scaffold for both index build passes. Reuses the
/// `workspace_files` map so each file's `FileScope` is built once.
fn walk_files<F>(inputs: &WalkInputs<'_>, index: &mut WorkspaceTypeIndex, mut visit: F)
where
    F: FnMut(&mut WorkspaceTypeIndex, &BuildContext<'_>, &syn::File),
{
    for (path, ast) in inputs.files {
        if inputs.cfg_test_files.contains(*path) {
            continue;
        }
        let Some(file) = inputs.workspace_files.get(*path) else {
            continue;
        };
        let ctx = BuildContext {
            file,
            workspace_files: inputs.workspace_files,
            transparent_wrappers: inputs.transparent_wrappers,
            type_aliases: inputs.type_aliases,
        };
        visit(index, &ctx, ast);
    }
}
