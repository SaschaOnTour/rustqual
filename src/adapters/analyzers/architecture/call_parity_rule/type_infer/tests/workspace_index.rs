//! Integration tests for `WorkspaceTypeIndex` building.
//!
//! Covers struct-field, method-return, and free-fn-return collection
//! across single- and multi-file workspaces plus the cfg-test skip
//! behaviour.

use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::{
    collect_local_symbols_scoped, LocalSymbols,
};
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    build_workspace_type_index, CanonicalType, WorkspaceIndexInputs,
};
use crate::adapters::shared::use_tree::gather_alias_map;
use std::collections::{HashMap, HashSet};

fn parse_file(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse file")
}

struct WsFixture {
    parsed: Vec<(String, syn::File)>,
    aliases: HashMap<String, HashMap<String, Vec<String>>>,
    local_symbols: HashMap<String, LocalSymbols>,
}

fn fixture(entries: &[(&str, &str)]) -> WsFixture {
    let mut parsed = Vec::new();
    let mut aliases = HashMap::new();
    let mut local_symbols = HashMap::new();
    for (path, src) in entries {
        let ast = parse_file(src);
        aliases.insert(path.to_string(), gather_alias_map(&ast));
        local_symbols.insert(path.to_string(), collect_local_symbols_scoped(&ast));
        parsed.push((path.to_string(), ast));
    }
    WsFixture {
        parsed,
        aliases,
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &HashSet::new(),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/session.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/context.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
    let field = index.struct_field("crate::app::context::Ctx", "inner");
    assert_eq!(
        field,
        Some(&CanonicalType::path(["crate", "app", "context", "Inner"]))
    );
}

#[test]
fn test_tuple_struct_is_not_indexed() {
    let fix = fixture(&[("src/app/foo.rs", "pub struct Id(pub String);")]);
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/foo.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/foo.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/session.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/session.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
fn test_method_with_unit_return_is_not_indexed() {
    let fix = fixture(&[(
        "src/app/foo.rs",
        r#"
        pub struct S;
        impl S { pub fn bump(&self) {} }
        "#,
    )]);
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/foo.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/foo.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/foo.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/make.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/make.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
    // Generic T has no alias/local-symbol entry → Opaque → skipped.
    assert!(index.fn_returns.is_empty());
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/mod.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/mod.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
    // Pre-fix: `Session` was looked up against the file's top-level
    // local symbols (which only contained `inner`), so the return type
    // resolved to `Opaque` and `make` was dropped from the index.
    // With per-mod-scope resolution `Session` is found at scope `[inner]`
    // and the return canonical is `crate::app::inner::Session`.
    assert_eq!(
        index.fn_return("crate::app::inner::make"),
        Some(&CanonicalType::path([
            "crate", "app", "inner", "Session"
        ]))
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/mod.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/foo.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &cfg_test,
        crate_root_modules: &crate_roots(&["src/app/foo.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/ports.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/foo.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/foo.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/foo.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/ports/handler.rs", "src/app/session.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
    let index = build_workspace_type_index(&WorkspaceIndexInputs {
        files: &borrowed(&fix),
        aliases_per_file: &fix.aliases,
        local_symbols_per_file: &fix.local_symbols,
        cfg_test_files: &HashSet::new(),
        crate_root_modules: &crate_roots(&["src/app/session.rs", "src/app/impls.rs"]),
        transparent_wrappers: &HashSet::new(),
    });
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
