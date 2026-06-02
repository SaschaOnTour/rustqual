use super::*;

#[test]
fn test_impl_trait_local_send_named_trait_resolves_not_skipped() {
    // `use crate::ports::Send;` then `impl Send` (or `dyn Send`) refers
    // to a workspace trait that happens to share its name with the std
    // marker. The marker-trait skip used to discard it based on the raw
    // last segment, dropping the bound and leaving downstream method
    // dispatch (`h.handle()`) unresolved. After alias canonicalisation
    // the path resolves to `crate::ports::Send`, which is *not*
    // stdlib-prefixed and therefore not a marker.
    let resolved = resolve_aliased(
        "impl Send",
        "src/app/mod.rs",
        &[("Send", &["crate", "ports", "Send"])],
        &[],
    );
    assert_eq!(
        resolved,
        CanonicalType::TraitBound(vec![vec![
            "crate".to_string(),
            "ports".to_string(),
            "Send".to_string(),
        ]]),
        "workspace `Send`-named trait must resolve to its canonical path, \
         not be discarded as a std marker; got {resolved:?}",
    );
}

#[test]
fn test_impl_trait_bare_std_send_marker_still_skipped() {
    // Regression guard: the canonicalisation-first refactor must NOT
    // start treating bare `Send` / `Sync` / `Debug` as workspace
    // traits. With no alias for `Send` in scope, the bound stays
    // unresolvable, and the conservative "single-segment marker"
    // fallback still skips it. `impl Handler + Send` therefore picks
    // up `Handler`, not the marker.
    let resolved = resolve_in("impl Handler + Send", "src/app/mod.rs", &["Handler"], &[]);
    assert_eq!(
        resolved,
        CanonicalType::TraitBound(vec![vec![
            "crate".to_string(),
            "app".to_string(),
            "Handler".to_string(),
        ]]),
        "bare std-marker `Send` must still be skipped so dispatch picks \
         up the local `Handler`; got {resolved:?}",
    );
}

#[test]
fn test_impl_trait_external_aliased_bound_skipped_workspace_bound_wins() {
    // `use serde::Serialize;` then `impl Serialize + Handler`. The
    // first bound canonicalises to the external path
    // `["serde", "Serialize"]`. Trait dispatch only knows the
    // workspace, so accepting an external bound would shadow the
    // later workspace `Handler` and leave `make().handle()`
    // unresolved. External aliased bounds must be skipped just like
    // fully-qualified `serde::Serialize` already is (canonicalisation
    // returns `None` for the un-aliased external form).
    let resolved = resolve_aliased(
        "impl Serialize + Handler",
        "src/app/mod.rs",
        &[("Serialize", &["serde", "Serialize"])],
        &["Handler"],
    );
    assert_eq!(
        resolved,
        CanonicalType::TraitBound(vec![vec![
            "crate".to_string(),
            "app".to_string(),
            "Handler".to_string(),
        ]]),
        "external aliased bound `serde::Serialize` must be skipped so \
         workspace `Handler` is picked; got {resolved:?}",
    );
}

#[test]
fn test_impl_aliased_future_resolves_to_future_with_output() {
    // `use std::future::Future as Fut;` then `impl Fut<Output = Session>`.
    // The raw bound leaf is `Fut`, but the canonicalised path is
    // `std::future::Future`, so the bound must still shape the
    // result as `Future(Session)` — otherwise `.await` on the
    // return type doesn't unwrap and downstream method dispatch
    // (e.g. `make().await.diff()`) stays unresolved.
    let resolved = resolve_aliased(
        "impl Fut<Output = Session>",
        "src/app/mod.rs",
        &[("Fut", &["std", "future", "Future"])],
        &["Session"],
    );
    let expected_output = CanonicalType::path(["crate", "app", "Session"]);
    assert_eq!(
        resolved,
        CanonicalType::Future(Box::new(expected_output)),
        "aliased Future bound must canonicalise to Future(Session); got {resolved:?}",
    );
}

/// Per-file scope inputs the cross-module alias test owns. `FileScope`
/// holds borrows, so the owning storage stays here and `as_scope`
/// produces a fresh borrow at call sites.
struct ScopeInputs {
    path: String,
    alias_map: crate::adapters::shared::use_tree::AliasMap,
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
            workspace_module_paths: None,
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
        AliasTarget::relative(vec![
            "crate".to_string(),
            "domain".to_string(),
            "Wrap".to_string(),
        ]),
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
            generic_params: None,
            reexports: None,
        },
    );
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "Session"]),
        "alias generic args must resolve at the use-site, got {resolved:?}"
    );
}

#[test]
fn unbounded_generic_param_shadows_same_named_workspace_symbol() {
    // `fn f<Q>(...)` with NO bound on Q. Inside the body, a path like
    // `Q` must resolve to `Opaque` (the fn-scoped param shadows any
    // workspace symbol of the same name), not to the workspace
    // canonical for a top-level type named `Q`. Without explicit
    // shadowing, `resolve_generic_path` would fall through to
    // `canonicalise_type_segments_in_scope` and resolve `Q` to
    // `crate::Q`, producing wrong method-dispatch edges.
    // Unbounded `<Q>` with `Q` also in local symbols: the param must
    // shadow the workspace symbol so `Q` resolves to Opaque, not crate::Q.
    let resolved = resolve_with_unbounded_param("Q", "src/app/runner.rs", &["Q"], "Q");
    assert_eq!(
        resolved,
        CanonicalType::Opaque,
        "unbounded generic param `Q` must shadow same-named workspace \
         symbol — falling through to canonicalisation would resolve \
         to `crate::Q` and produce wrong dispatch edges, got {resolved:?}"
    );
}

#[test]
fn unbounded_generic_param_shadowing_does_not_affect_unrelated_path() {
    // Sanity: the shadowing fix must not affect a workspace symbol
    // whose name is NOT in `generic_params`. `Session` still resolves
    // to the workspace canonical even when `Q` is in the param map.
    // `Q` is the generic param, but the resolved path is `Session` — not
    // in generic_params — so it must still resolve to the workspace canonical.
    let resolved = resolve_with_unbounded_param("Session", "src/app/session.rs", &["Session"], "Q");
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"]),
        "path NOT in generic_params must still resolve normally, got {resolved:?}"
    );
}
