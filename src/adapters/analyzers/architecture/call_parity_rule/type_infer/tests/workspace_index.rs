//! Integration tests for `WorkspaceTypeIndex` building.
//!
//! Covers struct-field, method-return, and free-fn-return collection
//! across single- and multi-file workspaces plus the cfg-test skip
//! behaviour.

use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::{
    build_workspace_files_map, collect_local_symbols_scoped, LocalSymbols, WorkspaceFilesInputs,
};
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    build_workspace_type_index, CanonicalType, WorkspaceIndexInputs,
};
use crate::adapters::shared::use_tree::{
    gather_alias_map, gather_alias_map_scoped, ScopedAliasMap,
};
use std::collections::{HashMap, HashSet};

fn parse_file(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse file")
}

struct WsFixture {
    parsed: Vec<(String, syn::File)>,
    aliases: HashMap<String, HashMap<String, Vec<String>>>,
    aliases_scoped: HashMap<String, ScopedAliasMap>,
    local_symbols: HashMap<String, LocalSymbols>,
}

fn fixture(entries: &[(&str, &str)]) -> WsFixture {
    let mut parsed = Vec::new();
    let mut aliases = HashMap::new();
    let mut aliases_scoped = HashMap::new();
    let mut local_symbols = HashMap::new();
    for (path, src) in entries {
        let ast = parse_file(src);
        aliases.insert(path.to_string(), gather_alias_map(&ast));
        aliases_scoped.insert(path.to_string(), gather_alias_map_scoped(&ast));
        local_symbols.insert(path.to_string(), collect_local_symbols_scoped(&ast));
        parsed.push((path.to_string(), ast));
    }
    WsFixture {
        parsed,
        aliases,
        aliases_scoped,
        local_symbols,
    }
}

fn borrowed(f: &WsFixture) -> Vec<(&str, &syn::File)> {
    f.parsed.iter().map(|(p, a)| (p.as_str(), a)).collect()
}

fn crate_roots(paths: &[&str]) -> HashSet<String> {
    paths
        .iter()
        .filter_map(|p| {
            let rest = p.strip_prefix("src/")?;
            let first = rest.split('/').next()?;
            let name = first.strip_suffix(".rs").unwrap_or(first);
            if matches!(name, "lib" | "main") {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

// ── Empty / trivial ──────────────────────────────────────────────

#[test]
fn test_empty_workspace_produces_empty_index() {
    let fix = fixture(&[]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &HashSet::new();
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert!(index.struct_fields.is_empty());
    assert!(index.method_returns.is_empty());
    assert!(index.fn_returns.is_empty());
}

// ── struct_fields ────────────────────────────────────────────────

#[test]
fn test_struct_with_named_field_is_indexed() {
    // Field type must be a workspace-local type — stdlib `String` would
    // resolve to `Opaque` (correct — stdlib isn't in our index) and get
    // skipped by `record_field`.
    let fix = fixture(&[(
        "src/app/session.rs",
        r#"
        pub struct Id;
        pub struct Session { pub id: Id }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/session.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let field = index.struct_field("crate::app::session::Session", "id");
    assert_eq!(
        field,
        Some(&CanonicalType::path(["crate", "app", "session", "Id"]))
    );
}

#[test]
fn test_struct_field_with_arc_is_stripped() {
    let fix = fixture(&[(
        "src/app/context.rs",
        r#"
        pub struct Inner { pub v: u8 }
        pub struct Ctx { pub inner: std::sync::Arc<Inner> }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/context.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let field = index.struct_field("crate::app::context::Ctx", "inner");
    assert_eq!(
        field,
        Some(&CanonicalType::path(["crate", "app", "context", "Inner"]))
    );
}

#[test]
fn test_tuple_struct_is_not_indexed() {
    let fix = fixture(&[("src/app/foo.rs", "pub struct Id(pub String);")]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/foo.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert!(index.struct_fields.is_empty());
}

#[test]
fn test_struct_field_with_opaque_type_is_skipped() {
    let fix = fixture(&[(
        "src/app/foo.rs",
        r#"
        pub struct Ctx { pub x: external_crate::Unknown }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/foo.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert!(index.struct_fields.is_empty());
}

// ── method_returns ───────────────────────────────────────────────

#[test]
fn test_inherent_method_with_concrete_return() {
    let fix = fixture(&[(
        "src/app/session.rs",
        r#"
        pub struct Session;
        pub struct Response;
        impl Session {
            pub fn diff(&self) -> Response { Response }
        }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/session.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let ret = index.method_return("crate::app::session::Session", "diff");
    assert_eq!(
        ret,
        Some(&CanonicalType::path([
            "crate", "app", "session", "Response"
        ]))
    );
}

#[test]
fn test_method_returning_result_wraps() {
    let fix = fixture(&[(
        "src/app/session.rs",
        r#"
        pub struct Session;
        pub struct Response;
        pub struct Error;
        impl Session {
            pub fn diff(&self) -> Result<Response, Error> { unimplemented!() }
        }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/session.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let ret = index
        .method_return("crate::app::session::Session", "diff")
        .expect("method indexed");
    match ret {
        CanonicalType::Result(inner) => assert_eq!(
            **inner,
            CanonicalType::path(["crate", "app", "session", "Response"])
        ),
        other => panic!("expected Result(_), got {:?}", other),
    }
}

#[test]
fn test_method_returning_result_self_substitutes_inner() {
    // `Session::open() -> Result<Self, Error>` must store
    // `Result<Session>`, not `Result<Opaque>`. Without nested-Self
    // substitution, downstream chains like
    // `Session::open().unwrap().diff()` lose the receiver type at
    // `.unwrap()` and fall back to `<method>:diff`.
    let fix = fixture(&[(
        "src/app/session.rs",
        r#"
        pub struct Session;
        pub struct Error;
        impl Session {
            pub fn open() -> Result<Self, Error> { unimplemented!() }
        }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/session.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let ret = index
        .method_return("crate::app::session::Session", "open")
        .expect("method indexed");
    let session = CanonicalType::path(["crate", "app", "session", "Session"]);
    assert_eq!(
        ret,
        &CanonicalType::Result(Box::new(session)),
        "Result<Self, _> must store Result<Session>, got {ret:?}"
    );
}

#[test]
fn test_method_with_unit_return_is_not_indexed() {
    let fix = fixture(&[(
        "src/app/foo.rs",
        r#"
        pub struct S;
        impl S { pub fn bump(&self) {} }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/foo.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert!(index.method_returns.is_empty());
}

#[test]
fn test_method_with_impl_trait_return_is_not_indexed() {
    let fix = fixture(&[(
        "src/app/foo.rs",
        r#"
        pub struct S;
        impl S { pub fn iter(&self) -> impl Iterator<Item = u8> { std::iter::empty() } }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/foo.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert!(index.method_returns.is_empty());
}

#[test]
fn test_trait_impl_method_is_indexed_by_receiver_type() {
    let fix = fixture(&[(
        "src/app/foo.rs",
        r#"
        pub struct S;
        pub struct T;
        pub trait Convert { fn to(&self) -> T; }
        impl Convert for S { fn to(&self) -> T { T } }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/foo.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    // Keyed by the concrete receiver type S, NOT by the trait.
    let ret = index.method_return("crate::app::foo::S", "to");
    assert_eq!(
        ret,
        Some(&CanonicalType::path(["crate", "app", "foo", "T"]))
    );
}

// ── fn_returns ───────────────────────────────────────────────────

#[test]
fn test_free_fn_return_is_indexed() {
    let fix = fixture(&[(
        "src/app/make.rs",
        r#"
        pub struct Session;
        pub fn make_session() -> Session { Session }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/make.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let ret = index.fn_return("crate::app::make::make_session");
    assert_eq!(
        ret,
        Some(&CanonicalType::path(["crate", "app", "make", "Session"]))
    );
}

#[test]
fn test_generic_return_type_is_opaque_and_not_indexed() {
    let fix = fixture(&[(
        "src/app/make.rs",
        r#"
        pub fn get<T>() -> T { unimplemented!() }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/make.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    // Generic T has no alias/local-symbol entry → Opaque → skipped.
    assert!(index.fn_returns.is_empty());
}

#[test]
fn fn_generic_param_return_does_not_collide_with_same_named_workspace_type() {
    // `pub struct Q;` plus `pub fn get<Q>() -> Q` — the workspace
    // type `Q` shares the leading segment with the fn-scoped generic
    // param `Q`. Without threading the fn's generics into the
    // workspace-index resolve context, `resolve_type(Q)` falls through
    // to the canonicaliser and resolves `Q` to `crate::app::make::Q`
    // (the struct), making `fn_returns["...::get"] = crate::app::make::Q`.
    // Downstream `get::<Session>().diff()` inference would then short-
    // circuit on the (wrong) concrete return type instead of using the
    // turbofish.
    //
    // Expected: the fn-scoped param shadows the workspace symbol,
    // resolution yields `Opaque`, and the entry is not indexed.
    let fix = fixture(&[(
        "src/app/make.rs",
        r#"
        pub struct Q;
        pub fn get<Q>() -> Q { unimplemented!() }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/make.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert_eq!(
        index.fn_return("crate::app::make::get"),
        None,
        "fn-scoped generic param `Q` must shadow workspace struct `Q` \
         and yield Opaque (skipped from index). Got: {:?}",
        index.fn_returns,
    );
}

#[test]
fn method_generic_param_return_does_not_collide_with_same_named_workspace_type() {
    // Same shadowing concern, method-level: `impl Service { fn get<Q>(&self) -> Q }`
    // where the workspace also has `pub struct Q;`. Without threading
    // the method's generic params into the resolve context, the return
    // type resolves to the workspace struct and poisons the
    // `method_returns` map.
    let fix = fixture(&[(
        "src/app/make.rs",
        r#"
        pub struct Q;
        pub struct Service;
        impl Service {
            pub fn get<Q>(&self) -> Q { unimplemented!() }
        }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/make.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert_eq!(
        index.method_return("crate::app::make::Service", "get"),
        None,
        "method-scoped generic param `Q` must shadow workspace struct \
         `Q` and yield Opaque (skipped from method_returns). Got: {:?}",
        index.method_returns,
    );
}

#[test]
fn bounded_fn_generic_param_return_carries_canonicalised_trait_bound() {
    // `pub fn make<Q: Handler>() -> Q` where `Handler` is in scope via
    // `use crate::ports::Handler;`. The fn-return entry must store a
    // canonicalised `TraitBound([["crate","ports","Handler"]])`, NOT
    // the raw single-segment `[["Handler"]]` — downstream
    // `trait_has_method` / anchor lookups key on canonical paths and
    // would silently miss the un-canonicalised form, dropping valid
    // trait-dispatch edges.
    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            "#,
        ),
        (
            "src/app/make.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn make<Q: Handler>() -> Q { unimplemented!() }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/ports/handler.rs", "src/app/make.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    // Acceptable outcomes: either skipped entirely (Opaque) or stored
    // as GenericParamBound with the canonical path. The forbidden
    // outcome is a bound carrying a raw single-segment `["Handler"]`.
    // (Note: bare generic-param returns now use `GenericParamBound`,
    // not `TraitBound` — `TraitBound` is reserved for `impl Trait` /
    // `dyn Trait` shapes that don't substitute under turbofish.)
    if let Some(ret) = index.fn_return("crate::app::make::make") {
        let canonical_bounds: Vec<Vec<String>> = vec![vec![
            "crate".to_string(),
            "ports".to_string(),
            "handler".to_string(),
            "Handler".to_string(),
        ]];
        match ret {
            CanonicalType::GenericParamBound { bounds, .. } => {
                assert_eq!(
                    bounds, &canonical_bounds,
                    "trait bound for `Q: Handler` must be canonicalised \
                     to `crate::ports::handler::Handler`, got {bounds:?}"
                );
            }
            CanonicalType::Opaque => {} // also acceptable
            other => panic!("unexpected return type for `make`: {other:?}"),
        }
    }
}

#[test]
fn bounded_fn_generic_param_return_does_not_block_turbofish_inference() {
    // `pub fn get<Q: Handler>() -> Q` indexed as TraitBound(Handler)
    // must NOT block turbofish-based concrete-type inference at the
    // call site `get::<Session>().diff()` where `diff` is an inherent
    // method on `Session` not declared on `Handler`. This is the
    // turbofish-overrides-bounded-generic-return path.
    //
    // Surfaced via the full inference pipeline rather than the index
    // alone — driven through `collect_canonical_calls`. The behavioural
    // assertion lives there; this test exists as a forward-ref guard.
    //
    // Fixture: 2 files. workspace has both `Session::diff` (inherent)
    // and `Handler::handle` (trait method). `get::<Session>().diff()`
    // must produce edge `Session::diff`, NOT `Handler::diff` (which
    // doesn't exist).
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            "#,
        ),
        (
            "src/app/session.rs",
            r#"
            pub struct Session;
            impl Session { pub fn diff(&self) {} }
            "#,
        ),
        (
            "src/app/make.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn get<Q: Handler>() -> Q { unimplemented!() }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&[
            "src/ports/handler.rs",
            "src/app/session.rs",
            "src/app/make.rs",
        ]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };

    // Build a use-site fn that calls `get::<Session>().diff()` and
    // assert the canonical-call set contains `Session::diff`.
    let use_site = parse_file(
        r#"
        use crate::app::make::get;
        use crate::app::session::Session;
        pub fn use_it() {
            get::<Session>().diff();
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/cli/use_site.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/cli/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    // Walk the file's `use_it` body via collect_canonical_calls.
    let body = match &use_site.items[2] {
        syn::Item::Fn(item_fn) => &item_fn.block,
        _ => panic!("expected fn item at index 2 of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: vec![],
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let session_diff = "crate::app::session::Session::diff";
    assert!(
        calls.contains(session_diff),
        "turbofish `get::<Session>().diff()` must resolve via the turbofish \
         type arg (Session), not via the generic param's trait bound \
         (Handler). Calls: {calls:?}"
    );
}

#[test]
fn method_generic_param_return_canonicalises_where_bound_on_impl_generic() {
    // `impl<Q> Service<Q> { fn current(&self) -> Q where Q: Handler }` —
    // the where-clause bound `Q: Handler` lives on the method's `where`
    // but references the impl-level generic `Q`. A method-level
    // generics extractor that only sees the method's own param list
    // misses it; `method_canonical_generics(sig, impl_generics, …)`
    // must extend bounds for outer-name predicates so the bound
    // survives canonicalisation.
    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            "#,
        ),
        (
            "src/app/service.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct Service<Q>(pub Q);
            impl<Q> Service<Q> {
                pub fn current(&self) -> Q where Q: Handler { unimplemented!() }
            }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/ports/handler.rs", "src/app/service.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    // Strict: the method's where-clause bound on the impl-level
    // generic MUST be captured by `method_canonical_generics`. If the
    // extractor pipeline drops the outer-name predicate, the param
    // has empty bounds and the return resolves to Opaque (no info).
    // Capturing it surfaces
    // `GenericParamBound { bounds: [canonical Handler], turbofish_index: None }`
    // so downstream `service.current().handle()` routes through the
    // trait anchor. Note `turbofish_index: None` because Q is impl-
    // level (substituted via receiver type, not by a method-call
    // turbofish on `current`).
    let ret = index
        .method_return("crate::app::service::Service", "current")
        .expect("method-level where-bound must surface as GenericParamBound, not be dropped");
    let canonical_bounds: Vec<Vec<String>> = vec![vec![
        "crate".to_string(),
        "ports".to_string(),
        "handler".to_string(),
        "Handler".to_string(),
    ]];
    // Impl-level Q: turbofish_index is None (substituted via receiver
    // type, not method-call turbofish).
    assert_eq!(
        ret,
        &CanonicalType::GenericParamBound {
            bounds: canonical_bounds,
            turbofish_index: None,
        },
        "method-level `where Q: Handler` on impl-level `Q` must produce \
         a canonicalised GenericParamBound (with no method-turbofish \
         position) on the method's return, got {ret:?}"
    );
}

#[test]
fn struct_generic_param_field_does_not_collide_with_same_named_workspace_type() {
    // Workspace has both `pub struct Q;` and a generic
    // `pub struct Container<Q> { pub item: Q }`. The field type `Q`
    // is the struct's own generic param, NOT a reference to the
    // workspace struct. Without threading the struct's generics into
    // the field-collector resolve context, the canonicaliser resolves
    // `Q` to `crate::app::make::Q` (the workspace struct) and
    // `struct_fields["Container"]["item"] = crate::app::make::Q`,
    // poisoning later `self.item.method()` resolution.
    //
    // Expected: struct's generic `Q` shadows the workspace struct,
    // the field resolves to `Opaque`, and the entry is dropped.
    let fix = fixture(&[(
        "src/app/make.rs",
        r#"
        pub struct Q;
        pub struct Container<Q> { pub item: Q }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/make.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert_eq!(
        index.struct_field("crate::app::make::Container", "item"),
        None,
        "struct-scoped generic param `Q` must shadow workspace struct \
         `Q` and yield Opaque (skipped from struct_fields). Got: {:?}",
        index.struct_fields,
    );
}

#[test]
fn impl_level_generic_param_return_does_not_collide_with_same_named_workspace_type() {
    // Impl-level generic: `impl<Q> Service<Q> { fn first(&self) -> Q }`
    // with the workspace also exposing `pub struct Q;`. The impl-level
    // `Q` must shadow the workspace struct for every method's return-
    // type resolution.
    let fix = fixture(&[(
        "src/app/make.rs",
        r#"
        pub struct Q;
        pub struct Service<Q>(pub Q);
        impl<Q> Service<Q> {
            pub fn first(&self) -> Q { unimplemented!() }
        }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/make.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert_eq!(
        index.method_return("crate::app::make::Service", "first"),
        None,
        "impl-level generic `Q` must shadow workspace struct `Q` for \
         every method's return resolution. Got: {:?}",
        index.method_returns,
    );
}

#[test]
fn test_fn_inside_inline_mod_keys_include_mod_name() {
    let fix = fixture(&[(
        "src/app/mod.rs",
        r#"
        pub struct Session;
        pub mod inner {
            use super::Session;
            pub fn make_session() -> Session { Session }
        }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/mod.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    // With inline-mod tracking the key is `crate::app::inner::make_session`,
    // matching how `inner::make_session()` canonicalises at a call site.
    assert!(
        index.fn_return("crate::app::inner::make_session").is_some(),
        "fn_returns = {:?}",
        index.fn_returns.keys().collect::<Vec<_>>()
    );
    // And the pre-fix key is absent — no duplicate shadow-registration.
    assert!(index.fn_return("crate::app::make_session").is_none());
}

#[test]
fn test_fn_inside_inline_mod_resolves_inner_return_type() {
    let fix = fixture(&[(
        "src/app/mod.rs",
        r#"
        pub mod inner {
            pub struct Session;
            pub fn make() -> Session { Session }
        }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/mod.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    // Pre-fix: `Session` was looked up against the file's top-level
    // local symbols (which only contained `inner`), so the return type
    // resolved to `Opaque` and `make` was dropped from the index.
    // With per-mod-scope resolution `Session` is found at scope `[inner]`
    // and the return canonical is `crate::app::inner::Session`.
    assert_eq!(
        index.fn_return("crate::app::inner::make"),
        Some(&CanonicalType::path(["crate", "app", "inner", "Session"]))
    );
}

#[test]
fn test_struct_field_inside_inline_mod_keys_include_mod_name() {
    let fix = fixture(&[(
        "src/app/mod.rs",
        r#"
        pub struct Session;
        pub mod inner {
            use super::Session;
            pub struct Ctx { pub session: Session }
        }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/mod.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert!(
        index
            .struct_field("crate::app::inner::Ctx", "session")
            .is_some(),
        "struct_fields = {:?}",
        index.struct_fields.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_fn_with_unit_return_is_not_indexed() {
    let fix = fixture(&[("src/app/foo.rs", "pub fn bump() {}")]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/foo.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert!(index.fn_returns.is_empty());
}

// ── cfg-test skip ────────────────────────────────────────────────

#[test]
fn test_cfg_test_file_is_skipped() {
    let fix = fixture(&[(
        "src/app/foo.rs",
        r#"
        pub struct S { pub x: u8 }
        impl S { pub fn get(&self) -> u8 { self.x } }
        pub fn build() -> S { S { x: 0 } }
        "#,
    )]);
    let mut cfg_test = HashSet::new();
    cfg_test.insert("src/app/foo.rs".to_string());
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &cfg_test;
        let roots = &crate_roots(&["src/app/foo.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert!(index.struct_fields.is_empty());
    assert!(index.method_returns.is_empty());
    assert!(index.fn_returns.is_empty());
}

// ── multi-file ────────────────────────────────────────────────────

// ── trait_methods / trait_impls ───────────────────────────────────

#[test]
fn test_trait_declaration_methods_are_indexed() {
    let fix = fixture(&[(
        "src/app/ports.rs",
        r#"
        pub trait Handler {
            fn handle(&self, msg: &str);
            fn can_handle(&self, msg: &str) -> bool;
        }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/ports.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    assert!(index.trait_has_method("crate::app::ports::Handler", "handle"));
    assert!(index.trait_has_method("crate::app::ports::Handler", "can_handle"));
    assert!(!index.trait_has_method("crate::app::ports::Handler", "missing"));
}

#[test]
fn test_trait_impl_is_indexed() {
    let fix = fixture(&[(
        "src/app/foo.rs",
        r#"
        pub struct MyImpl;
        pub trait Handler { fn handle(&self); }
        impl Handler for MyImpl { fn handle(&self) {} }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/foo.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let impls = index.impls_of_trait("crate::app::foo::Handler");
    assert!(impls.contains(&"crate::app::foo::MyImpl".to_string()));
}

#[test]
fn test_multiple_impls_of_same_trait_all_indexed() {
    let fix = fixture(&[(
        "src/app/foo.rs",
        r#"
        pub trait Handler { fn handle(&self); }
        pub struct A;
        pub struct B;
        pub struct C;
        impl Handler for A { fn handle(&self) {} }
        impl Handler for B { fn handle(&self) {} }
        impl Handler for C { fn handle(&self) {} }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/foo.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let impls = index.impls_of_trait("crate::app::foo::Handler");
    assert_eq!(impls.len(), 3);
}

#[test]
fn test_inherent_impl_does_not_populate_trait_impls() {
    let fix = fixture(&[(
        "src/app/foo.rs",
        r#"
        pub struct S;
        impl S { pub fn method(&self) {} }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/foo.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    // Inherent impl has no trait reference, so trait_impls stays empty.
    assert!(index.trait_impls.is_empty());
}

#[test]
fn test_trait_in_one_file_impl_in_another() {
    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/app/session.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct Session;
            impl Handler for Session { fn handle(&self) {} }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = HashSet::new();
        let roots = crate_roots(&["src/ports/handler.rs", "src/app/session.rs"]);
        let wraps = HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: &cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: &roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: &cfg_test,
            transparent_wrappers: &wraps,
        })
    };
    // Trait resolved via import alias.
    let impls = index.impls_of_trait("crate::ports::handler::Handler");
    assert!(impls.contains(&"crate::app::session::Session".to_string()));
}

#[test]
fn test_struct_in_one_file_impl_in_another() {
    let fix = fixture(&[
        (
            "src/app/session.rs",
            r#"
            pub struct Id;
            pub struct Session { pub id: Id }
            "#,
        ),
        (
            "src/app/impls.rs",
            r#"
            use crate::app::session::{Session, Id};
            impl Session {
                pub fn clone_id(&self) -> Id { Id }
            }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let index = {
        let cfg_test = HashSet::new();
        let roots = crate_roots(&["src/app/session.rs", "src/app/impls.rs"]);
        let wraps = HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: &cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: &roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: &cfg_test,
            transparent_wrappers: &wraps,
        })
    };
    // Struct indexed from its declaration file.
    assert!(index
        .struct_field("crate::app::session::Session", "id")
        .is_some());
    // Method indexed from its impl file, keyed on the resolved
    // self-type (`crate::app::session::Session` via alias map).
    assert_eq!(
        index.method_return("crate::app::session::Session", "clone_id"),
        Some(&CanonicalType::path(["crate", "app", "session", "Id"]))
    );
}

#[test]
fn record_trait_methods_captures_method_span() {
    // The trait method's source location (file + 1-based line) must
    // be captured at index-build time so anchor findings can carry a
    // real source line. Without this, anchor findings hard-code
    // line=0, which breaks suppression-window matching, the orphan
    // detector, and SARIF location reporting.
    let fix = fixture(&[(
        "src/ports/handler.rs",
        // Lines: 1=blank, 2=trait, 3=method-decl
        "\npub trait Handler {\n    fn handle(&self);\n}\n",
    )]);
    let borrowed_files = borrowed(&fix);
    let cfg_test = HashSet::new();
    let roots = HashSet::new();
    let wraps = HashSet::new();
    let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
        files: &borrowed_files,
        cfg_test_files: &cfg_test,
        aliases_per_file: &fix.aliases,
        aliases_scoped_per_file: &fix.aliases_scoped,
        local_symbols_per_file: &fix.local_symbols,
        crate_root_modules: &roots,
        workspace_module_paths: None,
    });
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed_files,
        workspace_files: &workspace_files,
        cfg_test_files: &cfg_test,
        transparent_wrappers: &wraps,
    });
    let loc = index
        .trait_method_location("crate::ports::handler::Handler", "handle")
        .expect("trait method location must be captured");
    assert_eq!(loc.file, "src/ports/handler.rs");
    assert_eq!(loc.line, 3);
}

#[test]
fn method_call_turbofish_overrides_bounded_generic_param_return() {
    // Symmetric to the free-fn `get::<Session>().diff()` case: a method
    // call `service.current::<Session>().diff()` where `current<Q: Handler>`
    // returns a bounded generic param. The method's index entry stores
    // `TraitBound([crate::ports::handler::Handler])` (useful for plain
    // `service.current().handle()` dispatch). When the call site adds
    // an explicit `::<Session>` turbofish on the METHOD, the turbofish
    // substitution must win — the trailing `.diff()` then resolves to
    // `Session::diff` (inherent), not to a non-existent
    // `Handler::diff`. This is the same TraitBound + turbofish override
    // rule, applied to method calls instead of free-fn calls.
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            "#,
        ),
        (
            "src/app/session.rs",
            r#"
            pub struct Session;
            impl Session { pub fn diff(&self) {} }
            "#,
        ),
        (
            "src/app/service.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct Service;
            impl Service {
                pub fn current<Q: Handler>(&self) -> Q { unimplemented!() }
            }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&[
            "src/ports/handler.rs",
            "src/app/session.rs",
            "src/app/service.rs",
        ]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    // `s: &Service` typed via sig param so the binding installs as
    // Path([Service]) — `let s = Service;` would install Opaque
    // because the legacy let-binding extractor only handles
    // `Expr::Call` initialisers. With Opaque, `s.current()` would
    // never reach the GenericParamBound-override path this test is
    // designed to exercise.
    let use_site = parse_file(
        r#"
        use crate::app::service::Service;
        use crate::app::session::Session;
        pub fn use_it(s: &Service) {
            s.current::<Session>().diff();
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/cli/use_site.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/cli/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::extract_signature_params;
    let (body, sig) = match &use_site.items[2] {
        syn::Item::Fn(item_fn) => (&item_fn.block, &item_fn.sig),
        _ => panic!("expected fn item at index 2 of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: extract_signature_params(sig),
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let session_diff = "crate::app::session::Session::diff";
    assert!(
        calls.contains(session_diff),
        "method-call turbofish `s.current::<Session>().diff()` must \
         resolve via the turbofish type arg (Session), not via the \
         method's generic-param trait bound (Handler). Calls: {calls:?}"
    );
}

#[test]
fn free_fn_impl_trait_return_turbofish_does_not_substitute_return() {
    // `fn make<T>() -> impl Handler` — the return type is `impl Handler`,
    // an OPAQUE type that doesn't reference `T`. Calling `make::<Session>()`
    // substitutes T=Session in the BODY, but the return is still
    // "some Handler", not Session. Critical contrast to the
    // `get<Q: Handler>() -> Q` case where Q IS the return type and
    // turbofish DOES substitute it.
    //
    // Bug: pre-split, both shapes were stored as `TraitBound(Handler)`
    // in the index, indistinguishable. `turbofish_substitute` would
    // fire on either and produce a false `Session::diff` edge for the
    // impl-Trait case. The split into `TraitBound` (impl/dyn Trait,
    // not substitutable) and `GenericParamBound` (bare param return,
    // substitutable) fixes this — only the latter triggers the
    // override.
    //
    // Required behaviour: `make::<Session>().diff()` must NOT produce
    // edge `Session::diff` (Session may not even implement Handler, and
    // even if it does, `impl Handler` is opaque from the caller's view).
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            "#,
        ),
        (
            "src/app/session.rs",
            r#"
            pub struct Session;
            impl Session { pub fn diff(&self) {} }
            "#,
        ),
        (
            "src/app/make.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn make<T>() -> impl Handler { unimplemented!() }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&[
            "src/ports/handler.rs",
            "src/app/session.rs",
            "src/app/make.rs",
        ]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let use_site = parse_file(
        r#"
        use crate::app::make::make;
        use crate::app::session::Session;
        pub fn use_it() {
            make::<Session>().diff();
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/cli/use_site.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/cli/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let body = match &use_site.items[2] {
        syn::Item::Fn(item_fn) => &item_fn.block,
        _ => panic!("expected fn item at index 2 of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: vec![],
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let phantom_session_diff = "crate::app::session::Session::diff";
    assert!(
        !calls.contains(phantom_session_diff),
        "`make::<Session>().diff()` where `make<T>() -> impl Handler` \
         must NOT produce edge `{phantom_session_diff}` — the return is \
         opaque `impl Handler`, not Session. Turbofish on T doesn't \
         substitute the return type. Calls: {calls:?}"
    );
}

#[test]
fn method_call_impl_trait_return_turbofish_does_not_substitute_return() {
    // Method-call analogue: `fn make_handler<T>(&self) -> impl Handler`.
    // `s.make_handler::<Session>().diff()` must NOT produce
    // `Session::diff` edge — same reasoning, the return is opaque.
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            "#,
        ),
        (
            "src/app/session.rs",
            r#"
            pub struct Session;
            impl Session { pub fn diff(&self) {} }
            "#,
        ),
        (
            "src/app/service.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct Service;
            impl Service {
                pub fn make_handler<T>(&self) -> impl Handler { unimplemented!() }
            }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&[
            "src/ports/handler.rs",
            "src/app/session.rs",
            "src/app/service.rs",
        ]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let use_site = parse_file(
        r#"
        use crate::app::service::Service;
        use crate::app::session::Session;
        pub fn use_it() {
            let s = Service;
            s.make_handler::<Session>().diff();
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/cli/use_site.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/cli/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let body = match &use_site.items[2] {
        syn::Item::Fn(item_fn) => &item_fn.block,
        _ => panic!("expected fn item at index 2 of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: vec![],
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let phantom_session_diff = "crate::app::session::Session::diff";
    assert!(
        !calls.contains(phantom_session_diff),
        "method-call `s.make_handler::<Session>().diff()` where \
         `make_handler<T>(&self) -> impl Handler` must NOT produce edge \
         `{phantom_session_diff}` — return is opaque `impl Handler`. \
         Calls: {calls:?}"
    );
}

#[test]
fn multi_generic_fn_turbofish_picks_correct_arg_for_returned_param() {
    // `fn get<A, Q: Handler>() -> Q` — A is the FIRST generic param,
    // Q is the SECOND and IS the return. Calling
    // `get::<Audit, Session>()` substitutes A=Audit and Q=Session.
    // The return is Q → Session. The bug: pre-fix `turbofish_substitute`
    // always picks the FIRST turbofish arg, so it would substitute
    // Audit instead of Session, then `.diff()` would route to
    // `Audit::diff` (false edge) or miss `Session::diff`.
    //
    // Required: GenericParamBound must carry the position of the
    // returned param in the call-site-substitutable generics list, so
    // turbofish substitution picks arg-at-position.
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            "#,
        ),
        (
            "src/app/audit.rs",
            r#"
            pub struct Audit;
            impl Audit { pub fn audit_method(&self) {} }
            "#,
        ),
        (
            "src/app/session.rs",
            r#"
            pub struct Session;
            impl Session { pub fn diff(&self) {} }
            "#,
        ),
        (
            "src/app/make.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn get<A, Q: Handler>() -> Q { unimplemented!() }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&[
            "src/ports/handler.rs",
            "src/app/audit.rs",
            "src/app/session.rs",
            "src/app/make.rs",
        ]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let use_site = parse_file(
        r#"
        use crate::app::make::get;
        use crate::app::audit::Audit;
        use crate::app::session::Session;
        pub fn use_it() {
            get::<Audit, Session>().diff();
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/cli/use_site.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/cli/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let body = match &use_site.items[3] {
        syn::Item::Fn(item_fn) => &item_fn.block,
        _ => panic!("expected fn item at index 3 of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: vec![],
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let session_diff = "crate::app::session::Session::diff";
    let phantom_audit_diff = "crate::app::audit::Audit::diff";
    assert!(
        calls.contains(session_diff),
        "`get::<Audit, Session>().diff()` where `get<A, Q: Handler>() -> Q` \
         must substitute Q=Session (the SECOND turbofish arg, matching Q's \
         position). Got calls: {calls:?}"
    );
    assert!(
        !calls.contains(phantom_audit_diff),
        "must NOT substitute Q=Audit (the FIRST turbofish arg, matching A's \
         position — but A is not the return). Got calls: {calls:?}"
    );
}

#[test]
fn wrapper_around_generic_param_return_substitutes_inner_via_turbofish() {
    // `fn get<Q: Handler>() -> Result<Q, E>` — return type is a Result
    // wrapping the generic-param Q. Calling `get::<Session>()` substitutes
    // Q=Session inside the wrapper, so `.unwrap()` produces a Session.
    // Then `.diff()` resolves to `Session::diff`. Bug: pre-fix
    // `turbofish_substitute` only fired when the whole inferred type
    // was GenericParamBound — it didn't recurse into Result/Option/
    // Future wrappers, so the Q inside `Result<Q, E>` was left
    // un-substituted. After `.unwrap()` peeled the Result, the turbofish
    // context was gone and `.diff()` couldn't resolve to Session::diff.
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            "#,
        ),
        (
            "src/app/session.rs",
            r#"
            pub struct Session;
            impl Session { pub fn diff(&self) {} }
            "#,
        ),
        (
            "src/app/make.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct MyErr;
            pub fn get<Q: Handler>() -> Result<Q, MyErr> { unimplemented!() }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&[
            "src/ports/handler.rs",
            "src/app/session.rs",
            "src/app/make.rs",
        ]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let use_site = parse_file(
        r#"
        use crate::app::make::get;
        use crate::app::session::Session;
        pub fn use_it() {
            get::<Session>().unwrap().diff();
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/cli/use_site.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/cli/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let body = match &use_site.items[2] {
        syn::Item::Fn(item_fn) => &item_fn.block,
        _ => panic!("expected fn item at index 2 of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: vec![],
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let session_diff = "crate::app::session::Session::diff";
    assert!(
        calls.contains(session_diff),
        "`get::<Session>().unwrap().diff()` where `get<Q: Handler>() -> \
         Result<Q, MyErr>` must substitute Q=Session INSIDE the Result \
         wrapper so `.unwrap()` yields Session and `.diff()` resolves to \
         {session_diff}. Got calls: {calls:?}"
    );
}

#[test]
fn method_call_wrapper_around_generic_param_return_substitutes_inner() {
    // Method-call variant of the wrapper-recursion case:
    // `fn current<Q: Handler>(&self) -> Result<Q, MyErr>` —
    // `s.current::<Session>().unwrap().diff()` must produce
    // `Session::diff` by recursing into the Result wrapper.
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::extract_signature_params;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            "#,
        ),
        (
            "src/app/session.rs",
            r#"
            pub struct Session;
            impl Session { pub fn diff(&self) {} }
            "#,
        ),
        (
            "src/app/service.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct MyErr;
            pub struct Service;
            impl Service {
                pub fn current<Q: Handler>(&self) -> Result<Q, MyErr> { unimplemented!() }
            }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&[
            "src/ports/handler.rs",
            "src/app/session.rs",
            "src/app/service.rs",
        ]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let use_site = parse_file(
        r#"
        use crate::app::service::Service;
        use crate::app::session::Session;
        pub fn use_it(s: &Service) {
            s.current::<Session>().unwrap().diff();
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/cli/use_site.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/cli/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let (body, sig) = match &use_site.items[2] {
        syn::Item::Fn(item_fn) => (&item_fn.block, &item_fn.sig),
        _ => panic!("expected fn item at index 2 of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: extract_signature_params(sig),
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let session_diff = "crate::app::session::Session::diff";
    assert!(
        calls.contains(session_diff),
        "`s.current::<Session>().unwrap().diff()` where \
         `current<Q: Handler>(&self) -> Result<Q, MyErr>` must substitute \
         Q=Session inside the Result wrapper so `.diff()` resolves to \
         {session_diff}. Got calls: {calls:?}"
    );
}

#[test]
fn vec_around_generic_param_return_substitutes_inner_via_turbofish() {
    // `fn get<Q: Handler>() -> Vec<Q>` — `resolve_type` normalises
    // `Vec<Q>` to `CanonicalType::Slice(GenericParamBound { ... })`.
    // Iteration via `for s in get::<Session>()` extracts the Slice's
    // inner element, so without recursing turbofish substitution
    // through `Slice`, `s`'s type stays `GenericParamBound([Handler])`
    // and `s.diff()` routes via Handler-anchor (Handler has no diff)
    // instead of producing the concrete `Session::diff` edge.
    //
    // Required: turbofish_substitute must recurse into Slice (and
    // Map, by analogy with `HashMap<K, Q>` → Map(Q)) the same way it
    // already recurses into Result / Option / Future.
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[
        (
            "src/ports/handler.rs",
            r#"
            pub trait Handler { fn handle(&self); }
            "#,
        ),
        (
            "src/app/session.rs",
            r#"
            pub struct Session;
            impl Session { pub fn diff(&self) {} }
            "#,
        ),
        (
            "src/app/make.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn get<Q: Handler>() -> Vec<Q> { unimplemented!() }
            "#,
        ),
    ]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&[
            "src/ports/handler.rs",
            "src/app/session.rs",
            "src/app/make.rs",
        ]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let use_site = parse_file(
        r#"
        use crate::app::make::get;
        use crate::app::session::Session;
        pub fn use_it() {
            for s in get::<Session>() {
                s.diff();
            }
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/cli/use_site.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/cli/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let body = match &use_site.items[2] {
        syn::Item::Fn(item_fn) => &item_fn.block,
        _ => panic!("expected fn item at index 2 of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: vec![],
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let session_diff = "crate::app::session::Session::diff";
    assert!(
        calls.contains(session_diff),
        "`for s in get::<Session>() {{ s.diff(); }}` where \
         `get<Q: Handler>() -> Vec<Q>` must substitute Q=Session inside \
         the Vec/Slice wrapper so the iterator binding yields Session \
         and `.diff()` resolves to {session_diff}. Got calls: {calls:?}"
    );
}

#[test]
fn absolute_leading_colon_path_is_not_shadowed_by_in_scope_generic() {
    // `::Q::method()` is an explicit absolute path (Rust 2018+: from
    // an extern crate root). Even when an in-scope fn-generic param
    // is named `Q`, the leading-colon form intentionally disambiguates
    // AWAY from the generic. Pre-fix, `generic_param_shadow` matched
    // purely on segment text (no leading_colon check), so the absolute
    // `::Q` got mis-resolved as the generic's `Q` (GenericParamBound)
    // instead of falling through to normal canonicalisation.
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::ParamInfo;
    use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::resolve::{
        resolve_type, ResolveContext,
    };
    use crate::adapters::shared::use_tree::ScopedAliasMap;

    // Sanity check on the fixture: `::Q` must parse with leading_colon set.
    let ty: syn::Type = syn::parse_str("::Q").expect("parse `::Q`");
    let leading_colon_set = matches!(&ty, syn::Type::Path(tp) if tp.path.leading_colon.is_some());
    assert!(
        leading_colon_set,
        "fixture invariant: `::Q` must parse with leading_colon set"
    );

    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Q".to_string());
    let roots = HashSet::new();
    let file_scope = FileScope {
        path: "src/app/runner.rs",
        alias_map: &alias_map,
        aliases_per_scope: &ScopedAliasMap::new(),
        local_symbols: &local,
        local_decl_scopes: &HashMap::new(),
        crate_root_modules: &roots,
        workspace_module_paths: None,
    };
    let mut generics: HashMap<String, ParamInfo> = HashMap::new();
    generics.insert(
        "Q".to_string(),
        ParamInfo {
            bounds: vec![vec![
                "crate".to_string(),
                "ports".to_string(),
                "Handler".to_string(),
            ]],
            turbofish_index: Some(0),
        },
    );
    let resolved = resolve_type(
        &ty,
        &ResolveContext {
            file: &file_scope,
            mod_stack: &[],
            type_aliases: None,
            transparent_wrappers: None,
            workspace_files: None,
            alias_param_subs: None,
            generic_params: Some(&generics),
        },
    );
    assert!(
        !matches!(resolved, CanonicalType::GenericParamBound { .. }),
        "`::Q` (leading_colon set) must NOT short-circuit through \
         generic_param_shadow — the absolute path is intentionally \
         disambiguated AWAY from the in-scope generic param Q. \
         Got: {resolved:?}"
    );
}

#[test]
fn absolute_leading_colon_call_path_does_not_shadow_to_trait_anchor() {
    // Sister to the resolve-side `::Q` shadowing test, but exercises
    // the CALL collector's path handling instead of the type
    // resolver. `visit_expr_call` strips `path.segments` to ident
    // strings and feeds them to `canonicalise_generic_param_path` —
    // pre-fix, that helper matched purely on segment text (no
    // leading_colon awareness), so `::Q::handle(x)` inside
    // `fn run<Q: Handler>(...)` got canonicalised to the trait-anchor
    // `Handler::handle` even though the explicit leading `::` is
    // the caller's disambiguation AWAY from the generic param.
    //
    // Required: when the call path has `leading_colon.is_some()`,
    // skip the generic-param canonicalisation branch — the absolute
    // path should fall through to normal path canonicalisation
    // (which will likely produce `<bare>:Q::handle` since `::Q` is
    // an extern-crate reference our analyzer can't resolve).
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::{
        item_canonical_generics, ParamInfo,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[(
        "src/ports/handler.rs",
        r#"
        pub trait Handler { fn handle(&self); }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/ports/handler.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let use_site = parse_file(
        r#"
        use crate::ports::handler::Handler;
        pub fn run<Q: Handler>(q: Q) {
            // Absolute path — explicitly NOT the generic Q.
            ::Q::handle(&q);
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/app/runner.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/app/runner.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let (body, sig) = match &use_site.items[1] {
        syn::Item::Fn(item_fn) => (&item_fn.block, &item_fn.sig),
        _ => panic!("expected fn item at index 1 of use_site"),
    };
    // Build the canonical generics map so `Q: Handler` is in scope
    // exactly the way the body collector sees it in production.
    let generics: std::collections::HashMap<String, ParamInfo> =
        item_canonical_generics(&sig.generics, &file_scope, &[]);
    // Sanity: Q must be in the generics map with Handler bound (else
    // the test is vacuous — without an in-scope Q, no shadowing could
    // happen even pre-fix).
    let q_info = generics
        .get("Q")
        .expect("fixture invariant: Q must be in generics map");
    assert!(
        !q_info.bounds.is_empty(),
        "fixture invariant: Q must have a resolved Handler bound"
    );
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: vec![],
        generic_params: generics,
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let trait_anchor = "crate::ports::handler::Handler::handle";
    assert!(
        !calls.contains(trait_anchor),
        "`::Q::handle(&q)` (leading_colon set) inside `fn run<Q: Handler>()` \
         must NOT canonicalise to the trait anchor `{trait_anchor}` — the \
         leading `::` is the caller's explicit disambiguation away from \
         the in-scope generic param Q. Got calls: {calls:?}"
    );
}

#[test]
fn absolute_leading_colon_type_path_does_not_route_to_same_named_workspace_type() {
    // Sister to the generic-param-shadow gate: even when no in-scope
    // generic matches `Q`, an absolute path `::Q` must NOT canonicalise
    // to a workspace `Q` via the fallback `canonicalise_type_segments_in_scope`.
    // Rust 2018+: `::Q` is from an extern crate root, so workspace
    // canonicalisation does not apply. Pre-fix, with `pub struct Q;`
    // in the workspace, `::Q` in a fn body resolves to
    // `crate::...::Q` via local-symbols / crate-roots lookup —
    // false-positive workspace edge.
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::resolve::{
        resolve_type, ResolveContext,
    };
    use crate::adapters::shared::use_tree::ScopedAliasMap;

    let ty: syn::Type = syn::parse_str("::Q").expect("parse `::Q`");
    let alias_map = HashMap::new();
    // Workspace has a local `Q` — exactly the false-positive trigger.
    let mut local = HashSet::new();
    local.insert("Q".to_string());
    let local_decl_scopes: HashMap<String, Vec<Vec<String>>> = {
        let mut m = HashMap::new();
        m.insert("Q".to_string(), vec![vec![]]);
        m
    };
    let roots = HashSet::new();
    let file_scope = FileScope {
        path: "src/app/runner.rs",
        alias_map: &alias_map,
        aliases_per_scope: &ScopedAliasMap::new(),
        local_symbols: &local,
        local_decl_scopes: &local_decl_scopes,
        crate_root_modules: &roots,
        workspace_module_paths: None,
    };
    let resolved = resolve_type(
        &ty,
        &ResolveContext {
            file: &file_scope,
            mod_stack: &[],
            type_aliases: None,
            transparent_wrappers: None,
            workspace_files: None,
            alias_param_subs: None,
            generic_params: None, // No generic in scope — purely a workspace-Q test
        },
    );
    // Must NOT be Path(crate::app::runner::Q) — the absolute leading
    // colon disambiguates AWAY from workspace symbols too, not just
    // generics.
    if let CanonicalType::Path(segs) = &resolved {
        assert!(
            !segs.contains(&"Q".to_string()) || segs.first().map(String::as_str) != Some("crate"),
            "`::Q` with `pub struct Q;` in the workspace must NOT canonicalise \
             to a workspace `Q` path. Got: {resolved:?}"
        );
    }
}

#[test]
fn absolute_leading_colon_call_path_does_not_route_to_same_named_workspace_fn() {
    // Sister test for the call collector: `::Q::handle()` with a
    // workspace `Q` available (local symbol or crate-root module)
    // must NOT produce a `crate::...::Q::handle` edge. The leading
    // colon disambiguates the call AWAY from workspace symbols too,
    // not just generic params.
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[(
        "src/app/q.rs",
        r#"
        // Workspace-local `Q` — the false-positive trigger.
        pub struct Q;
        impl Q { pub fn handle(&self) {} }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/app/q.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    // Use site: `Q` is in the crate root via the `q` module (local
    // symbol of nothing here — instead we'll check via crate-root
    // modules). To trigger the bug, we put a workspace-local Q in
    // the file's local_symbols by writing a file where Q is local.
    let use_site = parse_file(
        r#"
        pub struct Q;
        impl Q { pub fn handle(&self) {} }
        pub fn use_it() {
            ::Q::handle();
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    let crate_roots_set = collect_crate_root_modules(&[("src/cli/use_site.rs", &use_site)]);
    let file_scope = FileScope {
        path: "src/cli/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let body = match &use_site.items[2] {
        syn::Item::Fn(item_fn) => &item_fn.block,
        _ => panic!("expected fn item at index 2 of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: vec![],
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    // Must NOT produce a workspace edge — the leading colon means
    // extern. Acceptable canonicals: `<bare>:Q::handle` or similar.
    // Forbidden: any `crate::...::Q::handle` workspace canonical.
    let phantom_workspace_edge = "crate::cli::use_site::Q::handle";
    assert!(
        !calls.contains(phantom_workspace_edge),
        "`::Q::handle()` with workspace-local `Q` must NOT produce \
         workspace edge `{phantom_workspace_edge}` — the leading colon \
         disambiguates AWAY from workspace symbols. Got calls: {calls:?}"
    );
}

#[test]
fn generic_param_bound_with_leading_colon_does_not_route_to_workspace_trait() {
    // `fn run<Q: ::ports::handler::Handler>(q: Q) { Q::handle(&q); }`
    // — the trait bound on Q is an explicit absolute path `::ports::...`.
    // With a workspace-local `crate::ports::handler::Handler` trait in
    // the same crate, the bound's segments `["ports", "handler",
    // "Handler"]` would otherwise canonicalise to that workspace trait
    // and `Q::handle()` would emit a false `Handler::handle` anchor
    // edge. The leading colon must be preserved through bound
    // extraction so the workspace canonicalisation gate sees it.
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::item_canonical_generics;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[(
        "src/ports/handler.rs",
        r#"
        pub trait Handler { fn handle(&self); }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/ports/handler.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let use_site = parse_file(
        r#"
        pub fn run<Q: ::ports::handler::Handler>(q: Q) {
            Q::handle(&q);
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    // Include both files so `ports` is in crate_root_modules — that's
    // what makes the bound canonicalisable to the workspace trait
    // (the false-positive trigger).
    let crate_roots_set = collect_crate_root_modules(&[
        ("src/app/runner.rs", &use_site),
        ("src/ports/handler.rs", borrowed_files[0].1),
    ]);
    let file_scope = FileScope {
        path: "src/app/runner.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let (body, sig) = match &use_site.items[0] {
        syn::Item::Fn(item_fn) => (&item_fn.block, &item_fn.sig),
        _ => panic!("expected fn item at index 0 of use_site"),
    };
    let generics = item_canonical_generics(&sig.generics, &file_scope, &[]);
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: vec![],
        generic_params: generics,
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let phantom_anchor = "crate::ports::handler::Handler::handle";
    assert!(
        !calls.contains(phantom_anchor),
        "`Q::handle(&q)` where `Q: ::ports::handler::Handler` (extern \
         leading-colon bound) must NOT emit anchor edge `{phantom_anchor}` \
         — the leading colon disambiguates AWAY from the workspace \
         trait. Got calls: {calls:?}"
    );
}

#[test]
fn dyn_trait_with_leading_colon_does_not_route_to_workspace_trait_anchor() {
    // `fn use_it(x: &dyn ::ports::handler::Handler) { x.handle(); }`
    // — the `dyn` trait object's path is explicitly absolute. Without
    // propagating leading_colon through `resolve_bound_list`, the
    // bound canonicalises to `crate::ports::handler::Handler` and
    // `x.handle()` routes through the workspace trait-anchor.
    use crate::adapters::analyzers::architecture::call_parity_rule::calls::{
        collect_canonical_calls, FnContext,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::extract_signature_params;
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
        collect_crate_root_modules, collect_local_symbols,
    };
    use crate::adapters::shared::use_tree::gather_alias_map;

    let fix = fixture(&[(
        "src/ports/handler.rs",
        r#"
        pub trait Handler { fn handle(&self); }
        "#,
    )]);
    let borrowed_files = borrowed(&fix);
    let workspace_index = {
        let cfg_test = &HashSet::new();
        let roots = &crate_roots(&["src/ports/handler.rs"]);
        let wraps = &HashSet::new();
        let workspace_files = build_workspace_files_map(WorkspaceFilesInputs {
            files: &borrowed_files,
            cfg_test_files: cfg_test,
            aliases_per_file: &fix.aliases,
            aliases_scoped_per_file: &fix.aliases_scoped,
            local_symbols_per_file: &fix.local_symbols,
            crate_root_modules: roots,
            workspace_module_paths: None,
        });
        build_workspace_type_index(&WorkspaceIndexInputs {
            files: &borrowed_files,
            workspace_files: &workspace_files,
            cfg_test_files: cfg_test,
            transparent_wrappers: wraps,
        })
    };
    let use_site = parse_file(
        r#"
        pub fn use_it(x: &dyn ::ports::handler::Handler) {
            x.handle();
        }
        "#,
    );
    let alias_map = gather_alias_map(&use_site);
    let local_symbols = collect_local_symbols(&use_site);
    // Include both files so `ports` is in crate_root_modules — the
    // false-positive trigger requires the workspace trait to be
    // canonicalisable from the use_site's scope.
    let crate_roots_set = collect_crate_root_modules(&[
        ("src/app/use_site.rs", &use_site),
        ("src/ports/handler.rs", borrowed_files[0].1),
    ]);
    let file_scope = FileScope {
        path: "src/app/use_site.rs",
        alias_map: &alias_map,
        aliases_per_scope: &Default::default(),
        local_symbols: &local_symbols,
        local_decl_scopes: &Default::default(),
        crate_root_modules: &crate_roots_set,
        workspace_module_paths: None,
    };
    let (body, sig) = match &use_site.items[0] {
        syn::Item::Fn(item_fn) => (&item_fn.block, &item_fn.sig),
        _ => panic!("expected fn item at index 0 of use_site"),
    };
    let ctx = FnContext {
        file: &file_scope,
        mod_stack: &[],
        body,
        signature_params: extract_signature_params(sig),
        generic_params: std::collections::HashMap::new(),
        self_type: None,
        workspace_index: Some(&workspace_index),
        workspace_files: None,
    };
    let calls = collect_canonical_calls(&ctx);
    let phantom_anchor = "crate::ports::handler::Handler::handle";
    assert!(
        !calls.contains(phantom_anchor),
        "`x.handle()` where `x: &dyn ::ports::handler::Handler` (extern \
         leading-colon `dyn Trait`) must NOT emit anchor edge \
         `{phantom_anchor}`. Got calls: {calls:?}"
    );
}
