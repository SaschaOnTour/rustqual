//! Regression harness for call-parity receiver-type inference at the
//! `collect_canonical_calls` level. Each test sets up a workspace type-index
//! with a `Session` type + a `Ctx` struct, then runs a minimal fn body and
//! asserts the expected edge (or documented fall-back) appears. Split into
//! focused sub-files (each ≤ the SRP file-length cap); shared imports, the
//! `RegFixture` fixture, and the parse/run/index helpers live here and reach
//! the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
    collect_canonical_calls, FnContext,
};
pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    CanonicalType, WorkspaceTypeIndex,
};
pub(super) use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::collect_local_symbols;
pub(super) use crate::adapters::shared::use_tree::{
    gather_alias_map, gather_alias_map_scoped, AliasMap, ScopedAliasMap,
};
pub(super) use std::collections::{HashMap, HashSet};

mod fast_path_and_negative;
mod method_chains;
mod self_resolution;
mod trait_dispatch;
mod wrapper_peeling;
mod wrappers_and_aliases;

pub(super) const SESSION_PATH: &str = "crate::app::session::Session";
pub(super) const CTX_PATH: &str = "crate::app::Ctx";

/// RegFixture bundling a parsed file plus the resolution inputs
/// (`alias_map`, `local_symbols`, `crate_root_modules`) that
/// `collect_canonical_calls` expects.
pub(super) struct RegFixture {
    pub(super) file: syn::File,
    pub(super) alias_map: AliasMap,
    pub(super) local_symbols: HashSet<String>,
    pub(super) crate_roots: HashSet<String>,
}

pub(super) fn parse(src: &str) -> RegFixture {
    let file: syn::File = syn::parse_str(src).expect("parse fixture");
    let alias_map = gather_alias_map(&file);
    let local_symbols = collect_local_symbols(&file);
    RegFixture {
        file,
        alias_map,
        local_symbols,
        crate_roots: HashSet::new(),
    }
}

/// Pre-populated workspace index modelling a Session + Ctx shape.
pub(super) fn sample_session_index() -> WorkspaceTypeIndex {
    let session = CanonicalType::path(["crate", "app", "session", "Session"]);
    let response = CanonicalType::path(["crate", "app", "Response"]);
    let error = CanonicalType::path(["crate", "app", "Error"]);
    let mut index = WorkspaceTypeIndex::new();
    // Session::open() -> Result<Session, Error>
    index.insert_method_return(
        SESSION_PATH,
        "open",
        CanonicalType::Result(Box::new(session.clone())),
    );
    // Session::open_cwd() -> Result<Session, Error>
    index.insert_method_return(
        SESSION_PATH,
        "open_cwd",
        CanonicalType::Result(Box::new(session.clone())),
    );
    // Session::diff() -> Response
    index.insert_method_return(SESSION_PATH, "diff", response.clone());
    // Session::files() -> Response
    index.insert_method_return(SESSION_PATH, "files", response.clone());
    // Session::insert() -> Result<Response, Error>
    index.insert_method_return(
        SESSION_PATH,
        "insert",
        CanonicalType::Result(Box::new(response.clone())),
    );
    // Ctx { session: Session }
    index.insert_struct_field(CTX_PATH, "session", session);
    // Free fn make_session() -> Result<Session, Error>
    index.fn_returns.insert(
        "crate::app::make_session".to_string(),
        CanonicalType::Result(Box::new(CanonicalType::path([
            "crate", "app", "session", "Session",
        ]))),
    );
    let _ = error; // keep in scope if extensions need it
    index
}

pub(super) fn find_fn<'a>(file: &'a syn::File, name: &str) -> &'a syn::ItemFn {
    file.items
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(f) if f.sig.ident == name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fn {name} not in fixture"))
}

pub(super) fn sig_params(sig: &syn::Signature) -> Vec<(String, &syn::Type)> {
    sig.inputs
        .iter()
        .filter_map(|arg| match arg {
            syn::FnArg::Typed(pt) => match pt.pat.as_ref() {
                syn::Pat::Ident(pi) => Some((pi.ident.to_string(), pt.ty.as_ref())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// Run the fn body through `collect_canonical_calls` with the given
/// workspace index. Returns the set of canonical call targets.
pub(super) fn run(fx: &RegFixture, index: &WorkspaceTypeIndex, fn_name: &str) -> HashSet<String> {
    let f = find_fn(&fx.file, fn_name);
    run_with_self(fx, index, &f.sig, &f.block, None)
}

/// Run an `impl Type { fn name(&self, …) { … } }` body through the
/// collector with `self_type` set to crate-rooted segments. The
/// caller passes the canonical path so the test can bind to whichever
/// module the workspace index models for `Type` (e.g. Session lives
/// under `crate::app::session::*` in `sample_session_index()`).
pub(super) fn run_impl_method(
    fx: &RegFixture,
    index: &WorkspaceTypeIndex,
    type_name: &str,
    fn_name: &str,
    self_segs: Vec<String>,
) -> HashSet<String> {
    let (_, f) = find_impl_method(&fx.file, type_name, fn_name);
    run_with_self(fx, index, &f.sig, &f.block, Some(self_segs))
}

pub(super) fn run_with_self(
    fx: &RegFixture,
    index: &WorkspaceTypeIndex,
    sig: &syn::Signature,
    body: &syn::Block,
    self_type: Option<Vec<String>>,
) -> HashSet<String> {
    let ctx = FnContext {
        file: &FileScope {
            path: "src/cli/handlers.rs",
            alias_map: &fx.alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &fx.local_symbols,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &fx.crate_roots,
            workspace_module_paths: None,
        },
        mod_stack: &[],
        body,
        signature_params: sig_params(sig),
        generic_params: std::collections::HashMap::new(),
        self_type,
        workspace_index: Some(index),
        workspace_files: None,
        reexports: None,
    };
    collect_canonical_calls(&ctx)
}

pub(super) fn find_impl_method<'a>(
    file: &'a syn::File,
    type_name: &str,
    fn_name: &str,
) -> (Vec<String>, &'a syn::ImplItemFn) {
    file.items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Impl(i) => Some(i),
            _ => None,
        })
        .find_map(|item_impl| {
            let segs = impl_self_segments(item_impl)?;
            if segs.last().map(String::as_str) != Some(type_name) {
                return None;
            }
            item_impl.items.iter().find_map(|it| match it {
                syn::ImplItem::Fn(f) if f.sig.ident == fn_name => Some((segs.clone(), f)),
                _ => None,
            })
        })
        .unwrap_or_else(|| panic!("impl {type_name}::{fn_name} not in fixture"))
}

pub(super) fn impl_self_segments(item: &syn::ItemImpl) -> Option<Vec<String>> {
    let syn::Type::Path(p) = item.self_ty.as_ref() else {
        return None;
    };
    Some(
        p.path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect(),
    )
}

// ═══════════════════════════════════════════════════════════════════
// Positive: method-chain constructor patterns
// ═══════════════════════════════════════════════════════════════════
