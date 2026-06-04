use super::*;

#[test]
fn test_fn_inside_inline_mod_keys_include_mod_name() {
    // With inline-mod tracking the key is `crate::app::inner::make_session`,
    // matching how `inner::make_session()` canonicalises at a call site.
    let index = index_for(
        &[(
            "src/app/mod.rs",
            "pub struct Session;\npub mod inner {\nuse super::Session;\npub fn make_session() -> Session { Session }\n}",
        )],
        &["src/app/mod.rs"],
    );
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
    // With per-mod-scope resolution `Session` is found at scope `[inner]` and
    // the return canonical is `crate::app::inner::Session` (pre-fix it
    // resolved to Opaque against the file's top-level symbols and was dropped).
    let index = index_for(
        &[(
            "src/app/mod.rs",
            "pub mod inner {\npub struct Session;\npub fn make() -> Session { Session }\n}",
        )],
        &["src/app/mod.rs"],
    );
    assert_eq!(
        index.fn_return("crate::app::inner::make"),
        Some(&CanonicalType::path(["crate", "app", "inner", "Session"]))
    );
}

#[test]
fn test_struct_field_inside_inline_mod_keys_include_mod_name() {
    let index = index_for(
        &[(
            "src/app/mod.rs",
            "pub struct Session;\npub mod inner {\nuse super::Session;\npub struct Ctx { pub session: Session }\n}",
        )],
        &["src/app/mod.rs"],
    );
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
    let index = index_for(
        &[("src/app/foo.rs", "pub fn bump() {}")],
        &["src/app/foo.rs"],
    );
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
            reexports: None,
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
    let index = index_for(
        &[(
            "src/app/ports.rs",
            "pub trait Handler {\nfn handle(&self, msg: &str);\nfn can_handle(&self, msg: &str) -> bool;\n}",
        )],
        &["src/app/ports.rs"],
    );
    assert!(index.trait_has_method("crate::app::ports::Handler", "handle"));
    assert!(index.trait_has_method("crate::app::ports::Handler", "can_handle"));
    assert!(!index.trait_has_method("crate::app::ports::Handler", "missing"));
}

#[test]
fn test_trait_impl_is_indexed() {
    let index = index_for(
        &[(
            "src/app/foo.rs",
            "pub struct MyImpl;\npub trait Handler { fn handle(&self); }\nimpl Handler for MyImpl { fn handle(&self) {} }",
        )],
        &["src/app/foo.rs"],
    );
    let impls = index.impls_of_trait("crate::app::foo::Handler");
    assert!(impls.contains(&"crate::app::foo::MyImpl".to_string()));
}

#[test]
fn test_multiple_impls_of_same_trait_all_indexed() {
    let index = index_for(
        &[(
            "src/app/foo.rs",
            "pub trait Handler { fn handle(&self); }\npub struct A;\npub struct B;\npub struct C;\nimpl Handler for A { fn handle(&self) {} }\nimpl Handler for B { fn handle(&self) {} }\nimpl Handler for C { fn handle(&self) {} }",
        )],
        &["src/app/foo.rs"],
    );
    let impls = index.impls_of_trait("crate::app::foo::Handler");
    assert_eq!(impls.len(), 3);
}

#[test]
fn test_inherent_impl_does_not_populate_trait_impls() {
    // Inherent impl has no trait reference, so trait_impls stays empty.
    let index = index_for(
        &[(
            "src/app/foo.rs",
            "pub struct S;\nimpl S { pub fn method(&self) {} }",
        )],
        &["src/app/foo.rs"],
    );
    assert!(index.trait_impls.is_empty());
}

#[test]
fn test_trait_in_one_file_impl_in_another() {
    // Trait resolved via import alias.
    let index = index_for(
        &[
            (
                "src/ports/handler.rs",
                "pub trait Handler { fn handle(&self); }",
            ),
            (
                "src/app/session.rs",
                "use crate::ports::handler::Handler;\npub struct Session;\nimpl Handler for Session { fn handle(&self) {} }",
            ),
        ],
        &["src/ports/handler.rs", "src/app/session.rs"],
    );
    let impls = index.impls_of_trait("crate::ports::handler::Handler");
    assert!(impls.contains(&"crate::app::session::Session".to_string()));
}

#[test]
fn test_struct_in_one_file_impl_in_another() {
    let index = index_for(
        &[
            (
                "src/app/session.rs",
                "pub struct Id;\npub struct Session { pub id: Id }",
            ),
            (
                "src/app/impls.rs",
                "use crate::app::session::{Session, Id};\nimpl Session {\npub fn clone_id(&self) -> Id { Id }\n}",
            ),
        ],
        &["src/app/session.rs", "src/app/impls.rs"],
    );
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
    let index = index_for(
        &[(
            "src/ports/handler.rs",
            // Lines: 1=blank, 2=trait, 3=method-decl
            "\npub trait Handler {\n    fn handle(&self);\n}\n",
        )],
        &[],
    );
    let loc = index
        .trait_method_location("crate::ports::handler::Handler", "handle")
        .expect("trait method location must be captured");
    assert_eq!(loc.file, "src/ports/handler.rs");
    assert_eq!(loc.line, 3);
}
