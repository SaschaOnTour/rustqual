//! Unit tests for `syn::Type` → `CanonicalType` conversion.
//!
//! The `resolve` module is `pub(super)` — these tests live in the same
//! crate and reach it via the in-crate module path.

use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::canonical::CanonicalType;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::resolve::{
    resolve_type, ResolveContext,
};
use crate::adapters::shared::use_tree::ScopedAliasMap;
use std::collections::{HashMap, HashSet};

fn parse_type(src: &str) -> syn::Type {
    syn::parse_str(src).expect("parse type")
}

fn ctx<'a>(file: &'a FileScope<'a>) -> ResolveContext<'a> {
    ResolveContext {
        file,
        mod_stack: &[],
        type_aliases: None,
        transparent_wrappers: None,
        workspace_files: None,
        alias_param_subs: None,
    }
}

#[test]
fn test_bare_path_resolves_via_local_symbols() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Session");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/app/session.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_reference_type_strips_and_recurses() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    let ty = parse_type("&Session");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/app/session.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_result_wraps_inner() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Result<Session, Error>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/app/session.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    match resolved {
        CanonicalType::Result(inner) => {
            assert_eq!(
                *inner,
                CanonicalType::path(["crate", "app", "session", "Session"])
            );
        }
        other => panic!("expected Result(_), got {:?}", other),
    }
}

#[test]
fn test_option_wraps_inner() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("T".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Option<T>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/foo.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert!(matches!(resolved, CanonicalType::Option(_)));
}

#[test]
fn test_arc_is_stripped() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Arc<Session>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/app/session.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_nested_smart_pointers_strip_to_inner() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    // Only smart-pointer wrappers (`Arc` / `Box` / `Rc` / `Cow`) are
    // Deref-transparent, so nesting them still reaches the inner type.
    let ty = parse_type("Arc<Box<Session>>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/app/session.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_rwlock_is_not_peeled() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    // `RwLock::read()` returns a guard, not the inner value — peeling
    // it would synthesize bogus `Session::read` edges. Stays `Opaque`.
    let ty = parse_type("Arc<RwLock<Session>>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/app/session.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_vec_becomes_slice() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Handler".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Vec<Handler>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/foo.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert!(matches!(resolved, CanonicalType::Slice(_)));
}

#[test]
fn test_hashmap_keeps_value_type() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Handler".to_string());
    let roots = HashSet::new();
    let ty = parse_type("HashMap<String, Handler>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/foo.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    match resolved {
        CanonicalType::Map(inner) => {
            assert_eq!(*inner, CanonicalType::path(["crate", "foo", "Handler"]));
        }
        other => panic!("expected Map(_), got {:?}", other),
    }
}

#[test]
fn test_array_becomes_slice() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("T".to_string());
    let roots = HashSet::new();
    let ty = parse_type("[T; 4]");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/foo.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert!(matches!(resolved, CanonicalType::Slice(_)));
}

#[test]
fn test_slice_type_becomes_slice() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("T".to_string());
    let roots = HashSet::new();
    let ty = parse_type("&[T]");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/foo.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert!(matches!(resolved, CanonicalType::Slice(_)));
}

#[test]
fn test_trait_object_unresolved_is_opaque() {
    let alias_map = HashMap::new();
    let local = HashSet::new();
    let roots = HashSet::new();
    let ty = parse_type("Box<dyn Handler>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/foo.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    // Box<dyn T> → strip Box → dyn T — when T isn't resolvable (not in
    // local symbols / alias map / crate roots), stays Opaque.
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_trait_object_resolves_via_local_symbols() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Handler".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Box<dyn Handler>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/app/mod.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert_eq!(
        resolved,
        CanonicalType::TraitBound(vec![
            "crate".to_string(),
            "app".to_string(),
            "Handler".to_string(),
        ])
    );
}

#[test]
fn test_impl_trait_unresolved_is_opaque() {
    let alias_map = HashMap::new();
    let local = HashSet::new();
    let roots = HashSet::new();
    // `Iterator` isn't in local symbols / alias map — stays Opaque.
    let ty = parse_type("impl Iterator<Item = u8>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/foo.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_impl_trait_resolves_to_trait_bound() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Handler".to_string());
    let roots = HashSet::new();
    // `impl Handler` return-type resolves to `TraitBound(Handler)` so
    // trait-dispatch over-approximation can fire on the method call.
    let ty = parse_type("impl Handler + Send");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/app/mod.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert_eq!(
        resolved,
        CanonicalType::TraitBound(vec![
            "crate".to_string(),
            "app".to_string(),
            "Handler".to_string(),
        ])
    );
}

#[test]
fn test_unknown_external_path_is_opaque() {
    let alias_map = HashMap::new();
    let local = HashSet::new();
    let roots = HashSet::new();
    let ty = parse_type("external_crate::UnknownType");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/foo.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_aliased_path_resolves_via_alias_map() {
    let mut alias_map = HashMap::new();
    alias_map.insert(
        "Session".to_string(),
        vec![
            "crate".to_string(),
            "app".to_string(),
            "session".to_string(),
            "Session".to_string(),
        ],
    );
    let local = HashSet::new();
    let roots = HashSet::new();
    let ty = parse_type("Session");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/cli/handlers.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_future_wraps_output() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Response".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Future<Response>");
    let resolved = resolve_type(
        &ty,
        &ctx(&FileScope {
            path: "src/foo.rs",
            alias_map: &alias_map,
            aliases_per_scope: &ScopedAliasMap::new(),
            local_symbols: &local,
            local_decl_scopes: &HashMap::new(),
            crate_root_modules: &roots,
        }),
    );
    assert!(matches!(resolved, CanonicalType::Future(_)));
}

/// Per-file scope inputs the cross-module alias test owns. `FileScope`
/// holds borrows, so the owning storage stays here and `as_scope`
/// produces a fresh borrow at call sites.
struct ScopeInputs {
    path: String,
    alias_map: HashMap<String, Vec<String>>,
    aliases_per_scope: ScopedAliasMap,
    local_symbols: HashSet<String>,
    local_decl_scopes: HashMap<String, Vec<Vec<String>>>,
    crate_root_modules: HashSet<String>,
}

impl ScopeInputs {
    fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
            alias_map: HashMap::new(),
            aliases_per_scope: ScopedAliasMap::new(),
            local_symbols: HashSet::new(),
            local_decl_scopes: HashMap::new(),
            crate_root_modules: HashSet::new(),
        }
    }

    fn as_scope(&self) -> FileScope<'_> {
        FileScope {
            path: &self.path,
            alias_map: &self.alias_map,
            aliases_per_scope: &self.aliases_per_scope,
            local_symbols: &self.local_symbols,
            local_decl_scopes: &self.local_decl_scopes,
            crate_root_modules: &self.crate_root_modules,
        }
    }
}

#[test]
fn test_alias_generic_arg_resolves_at_use_site() {
    // `domain::type Wrap<T> = Arc<T>` consumed from `app` as
    // `Wrap<Session>`: the use-site arg `Session` must canonicalise
    // against `app`'s symbols, not against `domain`'s decl-site
    // scope, which doesn't know `Session`.
    use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::workspace_index::AliasDef;

    let domain = ScopeInputs::new("src/domain.rs");
    let mut app = ScopeInputs::new("src/app.rs");
    app.alias_map.insert(
        "Wrap".to_string(),
        vec![
            "crate".to_string(),
            "domain".to_string(),
            "Wrap".to_string(),
        ],
    );
    app.local_symbols.insert("Session".to_string());

    let mut workspace_files: HashMap<String, FileScope<'_>> = HashMap::new();
    workspace_files.insert("src/domain.rs".to_string(), domain.as_scope());

    let alias_target: syn::Type = syn::parse_str("Arc<T>").expect("parse alias target");
    let mut type_aliases: HashMap<String, AliasDef> = HashMap::new();
    type_aliases.insert(
        "crate::domain::Wrap".to_string(),
        AliasDef {
            params: vec!["T".to_string()],
            target: alias_target,
            decl_file: "src/domain.rs".to_string(),
            decl_mod_stack: Vec::new(),
        },
    );

    let app_scope = app.as_scope();
    let ty = parse_type("Wrap<Session>");
    let resolved = resolve_type(
        &ty,
        &ResolveContext {
            file: &app_scope,
            mod_stack: &[],
            type_aliases: Some(&type_aliases),
            transparent_wrappers: None,
            workspace_files: Some(&workspace_files),
            alias_param_subs: None,
        },
    );
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "Session"]),
        "alias generic args must resolve at the use-site, got {resolved:?}"
    );
}
