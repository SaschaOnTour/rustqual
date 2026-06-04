//! Unit tests for resolver internal predicates that the integration
//! `resolve_type` tests don't pin tightly: marker-trait detection
//! (`resolve_marker`), wrapper-name decision (`resolve_wrapper`), bare-
//! `Self` substitution (`self_subst`), single-ident projection
//! (`resolve_alias`), the `Future<Output=T>` assoc-type guard, and the
//! recursion-depth increment. Each isolates one boolean/arithmetic op.

use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::canonical::CanonicalType;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::resolve::{
    resolve_type, ResolveContext,
};
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::resolve_marker::is_marker_trait;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::resolve_wrapper::identify_wrapper_name;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::self_subst::substitute_bare_self;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::single_ident_of;
use crate::adapters::shared::use_tree::{AliasMap, ScopedAliasMap};
use std::collections::{HashMap, HashSet};

struct Scope {
    alias_map: AliasMap,
    aliases_per_scope: ScopedAliasMap,
    local_symbols: HashSet<String>,
    local_decl_scopes: HashMap<String, Vec<Vec<String>>>,
    crate_roots: HashSet<String>,
}

impl Scope {
    fn new() -> Self {
        Self {
            alias_map: HashMap::new(),
            aliases_per_scope: ScopedAliasMap::new(),
            local_symbols: HashSet::new(),
            local_decl_scopes: HashMap::new(),
            crate_roots: HashSet::new(),
        }
    }

    fn file_scope(&self) -> FileScope<'_> {
        FileScope {
            path: "src/app/x.rs",
            alias_map: &self.alias_map,
            aliases_per_scope: &self.aliases_per_scope,
            local_symbols: &self.local_symbols,
            local_decl_scopes: &self.local_decl_scopes,
            crate_root_modules: &self.crate_roots,
            workspace_module_paths: None,
        }
    }
}

fn ctx<'a>(file: &'a FileScope<'a>) -> ResolveContext<'a> {
    ResolveContext {
        file,
        mod_stack: &[],
        type_aliases: None,
        transparent_wrappers: None,
        workspace_files: None,
        alias_param_subs: None,
        generic_params: None,
        reexports: None,
    }
}

fn path_of(src: &str) -> syn::Path {
    syn::parse_str(src).expect("parse path")
}

// ── resolve_marker::is_marker_trait ─────────────────────────────────────

#[test]
fn bare_marker_trait_is_recognised() {
    // A bare `Send` (single segment, hits the std prelude) is a marker.
    // Pins `is_marker_trait -> false`, the `segs.len() == 1` `==`→`!=`,
    // and the `single_segment ||` `||`→`&&` — all three would flip this
    // to "not a marker".
    let scope = Scope::new();
    let file = scope.file_scope();
    assert!(
        is_marker_trait(&path_of("Send"), &ctx(&file)),
        "bare `Send` is a std marker trait"
    );
}

#[test]
fn bare_non_marker_trait_is_not_a_marker() {
    // A bare non-marker name (`Handler`) is a real bound, not a marker.
    // Pins the final `&& MARKER_TRAITS.contains` against `||` (which
    // would call any single-segment path a marker).
    let scope = Scope::new();
    let file = scope.file_scope();
    assert!(
        !is_marker_trait(&path_of("Handler"), &ctx(&file)),
        "bare `Handler` is a real bound"
    );
}

// ── resolve_wrapper::identify_wrapper_name ──────────────────────────────

#[test]
fn single_segment_non_wrapper_is_not_a_wrapper() {
    // `Foo` (single segment, not in WRAPPER_NAMES) is not a wrapper. Pins
    // `single && WRAPPER_NAMES.contains` against `||` (which would accept
    // any single-segment name). `Arc` is the positive control.
    let scope = Scope::new();
    let file = scope.file_scope();
    let c = ctx(&file);
    assert_eq!(
        identify_wrapper_name(&path_of("Foo"), "Foo", &c),
        None,
        "non-wrapper single ident → None"
    );
    assert_eq!(
        identify_wrapper_name(&path_of("Arc"), "Arc", &c).as_deref(),
        Some("Arc"),
        "Arc → wrapper"
    );
}

#[test]
fn explicit_stdlib_non_wrapper_is_not_a_wrapper() {
    // `std::Foo` reaches the explicit-stdlib branch; `Foo` isn't a
    // wrapper name. Pins `explicit_stdlib && WRAPPER_NAMES.contains`
    // against `||` (which would accept any std-prefixed path).
    let scope = Scope::new();
    let file = scope.file_scope();
    assert_eq!(
        identify_wrapper_name(&path_of("std::Foo"), "Foo", &ctx(&file)),
        None,
        "std-prefixed non-wrapper → None"
    );
}

// ── self_subst::substitute_bare_self ────────────────────────────────────

fn render(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream().to_string()
}

#[test]
fn substitute_rewrites_bare_self_but_not_associated_self() {
    // Bare `Self` is replaced with the impl segments; `Self::Output` (a
    // multi-segment associated path) is left untouched. Pins the
    // `qself.is_some() || segments.len() != 1` guard against `&&` (which
    // would treat `Self::Output` as bare and drop `Output`).
    let impl_segs = vec!["crate".to_string(), "app".to_string(), "Repo".to_string()];
    let bare: syn::Type = syn::parse_str("Self").unwrap();
    assert_eq!(
        render(&substitute_bare_self(&bare, &impl_segs)),
        render(&syn::parse_str::<syn::Type>("crate::app::Repo").unwrap()),
        "bare Self rewritten to impl path"
    );
    let assoc: syn::Type = syn::parse_str("Self::Output").unwrap();
    assert!(
        render(&substitute_bare_self(&assoc, &impl_segs)).contains("Output"),
        "Self::Output keeps its associated segment: {}",
        render(&substitute_bare_self(&assoc, &impl_segs))
    );
}

// ── resolve_alias::single_ident_of ──────────────────────────────────────

fn type_path(src: &str) -> syn::TypePath {
    match syn::parse_str::<syn::Type>(src).expect("parse type") {
        syn::Type::Path(tp) => tp,
        _ => panic!("expected type path"),
    }
}

#[test]
fn single_ident_of_rejects_multi_segment_paths() {
    // `a::b` is multi-segment → None; bare `T` → Some("T"). Pins the
    // `qself.is_some() || segments.len() != 1` guard against `&&` (which
    // would return `Some("a")` for the multi-segment path).
    assert_eq!(single_ident_of(&type_path("a::b")), None, "multi-segment");
    assert_eq!(
        single_ident_of(&type_path("T")).as_deref(),
        Some("T"),
        "single ident"
    );
}

// ── resolve: future Output-assoc guard + recursion depth ────────────────

fn resolve_src(ty_src: &str) -> CanonicalType {
    let scope = Scope::new();
    let file = scope.file_scope();
    resolve_type(&syn::parse_str(ty_src).expect("parse type"), &ctx(&file))
}

#[test]
fn future_requires_output_named_assoc_type() {
    // `Future<Output = T>` resolves to `Future<T>`; `Future<Item = T>`
    // (non-`Output` assoc, no positional arg) is unresolved → Opaque.
    // Pins the `a.ident == "Output"` match guard against `true` (which
    // would pick `Item`'s type).
    assert!(
        matches!(
            resolve_src("Future<Output = String>"),
            CanonicalType::Future(_)
        ),
        "Output assoc → Future"
    );
    assert_eq!(
        resolve_src("Future<Item = String>"),
        CanonicalType::Opaque,
        "non-Output assoc with no positional arg → Opaque"
    );
}

/// Count how many `Option` layers wrap the innermost type.
fn option_depth(mut ty: &CanonicalType) -> usize {
    let mut n = 0;
    while let CanonicalType::Option(inner) = ty {
        n += 1;
        ty = inner;
    }
    n
}

#[test]
fn resolution_depth_is_capped() {
    // Nesting deeper than MAX_RESOLVE_DEPTH (32) is truncated to Opaque.
    // Pins `depth_guarded`'s `depth + 1` against `depth * 1` (= no
    // increment, which would never hit the cap and resolve all 40 layers).
    let nested = format!("{}i32{}", "Option<".repeat(40), ">".repeat(40));
    assert_eq!(
        option_depth(&resolve_src(&nested)),
        32,
        "Option nesting capped at MAX_RESOLVE_DEPTH"
    );
}
