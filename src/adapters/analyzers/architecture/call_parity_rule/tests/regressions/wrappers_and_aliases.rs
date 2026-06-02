use super::*;

#[test]
fn user_wrapper_is_peeled_on_signature_param() {
    // Axum-style `fn h(State(db): State<Db>) { db.query() }`.
    // Stage 3: configure `State` as a transparent wrapper so the
    // inference peels it to reach `Db`, and `db.query()` resolves.
    // Note: our current `extract_pat_ident_name` handles `db: State<Db>`
    // pattern via `Pat::Ident` with type, not `State(db)` tuple-struct
    // destructuring — so we use the plain form here.
    let fx = parse(
        r#"
        use crate::app::Db;
        pub fn handle(db: State<Db>) {
            db.query();
        }
        "#,
    );
    let db = CanonicalType::path(["crate", "app", "Db"]);
    let mut index = WorkspaceTypeIndex::new();
    index.insert_method_return(
        "crate::app::Db",
        "query",
        CanonicalType::path(["crate", "app", "Rows"]),
    );
    // Register `State` as a transparent wrapper.
    index.transparent_wrappers.insert("State".to_string());
    let calls = run(&fx, &index, "handle");
    let _ = db;
    assert!(
        calls.contains("crate::app::Db::query"),
        "user-wrapper State<Db> should peel to Db, got {calls:?}"
    );
}

#[test]
fn user_wrapper_unconfigured_stays_unresolved() {
    // Same fixture but WITHOUT registering State as transparent. Falls
    // through to <method>:query.
    let fx = parse(
        r#"
        use crate::app::Db;
        pub fn handle(db: State<Db>) {
            db.query();
        }
        "#,
    );
    let index = WorkspaceTypeIndex::new();
    let calls = run(&fx, &index, "handle");
    assert!(
        calls.contains("<method>:query"),
        "unconfigured wrapper must not be peeled, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Stage 3: Type-Alias-Expansion
// ═══════════════════════════════════════════════════════════════════

#[test]
fn type_alias_expands_to_target_via_signature_param() {
    // `type DbRef = std::sync::Arc<Store>;` — `fn h(db: DbRef) { db.read() }`
    // Inference expands DbRef → Arc<Store> → Store (Arc wrapper peeled).
    // Store has a `read` method in our fixture.
    let fx = parse(
        r#"
        type DbRef = std::sync::Arc<Store>;
        pub fn handle(db: DbRef) {
            db.read();
        }
        "#,
    );
    let store = CanonicalType::path(["crate", "cli", "handlers", "Store"]);
    let mut index = WorkspaceTypeIndex::new();
    // Pre-populate the alias: `crate::cli::handlers::DbRef` → syn::Type
    // for `std::sync::Arc<Store>`.
    let aliased: syn::Type = syn::parse_str("std::sync::Arc<Store>").expect("parse alias target");
    // Non-generic alias — no params to substitute.
    index.type_aliases.insert(
        "crate::cli::handlers::DbRef".to_string(),
        crate::adapters::analyzers::architecture::call_parity_rule::type_infer::workspace_index::AliasDef {
            params: Vec::new(),
            target: aliased,
            decl_file: "src/cli/handlers.rs".to_string(),
            decl_mod_stack: Vec::new(),
        },
    );
    // Store::read() method.
    index.insert_method_return(
        "crate::cli::handlers::Store",
        "read",
        CanonicalType::path(["crate", "cli", "handlers", "Data"]),
    );
    // Include `DbRef` in local symbols so the alias key resolves.
    let mut fx = fx;
    fx.local_symbols.insert("DbRef".to_string());
    fx.local_symbols.insert("Store".to_string());
    let calls = run(&fx, &index, "handle");
    let _ = store;
    assert!(
        calls.contains("crate::cli::handlers::Store::read"),
        "type-alias should expand DbRef → Store, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Stage 2: Turbofish-as-Return-Type
// ═══════════════════════════════════════════════════════════════════

#[test]
fn turbofish_gives_concrete_return_type() {
    // `get::<Session>()` — generic fn with single turbofish type arg.
    // No fn_returns entry (generic returns are Opaque), so the
    // turbofish fallback fires and the return type is Session.
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = get::<Session>();
            s.diff();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "turbofish should resolve generic-ctor return type, got {calls:?}"
    );
}

#[test]
fn turbofish_on_type_method_is_not_overridden() {
    // `Vec::<u32>::new()` — turbofish is on the type segment, not the
    // method. Path has 2 segments, so the turbofish fallback doesn't
    // fire. `new` isn't in our index → falls through cleanly.
    let fx = parse(
        r#"
        pub fn cmd() {
            let v = Vec::<u32>::new();
            v.custom_method();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    // Important: we must NOT fabricate a `crate::…::u32::custom_method`
    // edge from the turbofish arg.
    assert!(
        calls.contains("<method>:custom_method"),
        "Vec::<T>::new() turbofish must not override, got {calls:?}"
    );
}

// ═══════════════════════════════════════════════════════════════════

#[test]
fn mixed_resolutions_in_single_body() {
    let fx = parse(
        r#"
        use crate::app::session::Session;
        pub fn cmd() {
            let s = Session::open().unwrap();
            s.diff();
            let x: u32 = 0;
            x.random();
            crate::app::make_session().unwrap().files();
        }
        "#,
    );
    let calls = run(&fx, &sample_session_index(), "cmd");
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "resolved: Session::diff missing, got {calls:?}"
    );
    assert!(
        calls.contains("crate::app::session::Session::files"),
        "resolved: Session::files missing, got {calls:?}"
    );
    assert!(
        calls.contains("<method>:random"),
        "unresolved: <method>:random expected, got {calls:?}"
    );
}
