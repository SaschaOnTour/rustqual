use super::*;

/// The 4-file workspace for the multi-generic turbofish test: a `Handler` port,
/// `Audit` + `Session` app types, and `get<A, Q: Handler>() -> Q` in `make`.
fn multi_generic_workspace() -> WorkspaceTypeIndex {
    index_for(
        &[
            (
                "src/ports/handler.rs",
                "pub trait Handler { fn handle(&self); }",
            ),
            (
                "src/app/audit.rs",
                "pub struct Audit;\nimpl Audit { pub fn audit_method(&self) {} }",
            ),
            (
                "src/app/session.rs",
                "pub struct Session;\nimpl Session { pub fn diff(&self) {} }",
            ),
            (
                "src/app/make.rs",
                "use crate::ports::handler::Handler;\npub fn get<A, Q: Handler>() -> Q { unimplemented!() }",
            ),
        ],
        &[
            "src/ports/handler.rs",
            "src/app/audit.rs",
            "src/app/session.rs",
            "src/app/make.rs",
        ],
    )
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
    let workspace_index = multi_generic_workspace();
    let calls = calls_from(
        &workspace_index,
        r#"
        use crate::app::make::get;
        use crate::app::audit::Audit;
        use crate::app::session::Session;
        pub fn use_it() {
            get::<Audit, Session>().diff();
        }
        "#,
        3,
    );
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

// `::Q::method()` is an explicit absolute path (Rust 2018+: from an extern
// crate root). Even when an in-scope fn-generic param is named `Q`, the
// leading-colon form intentionally disambiguates AWAY from the generic. Pre-fix,
// `generic_param_shadow` matched purely on segment text (no leading_colon
// check), so the absolute `::Q` got mis-resolved as the generic's `Q`
// (GenericParamBound) instead of falling through to normal canonicalisation.
// (The `::Q` parse invariant is its own test, `double_colon_q_parses…`, below.)
#[test]
fn absolute_leading_colon_path_is_not_shadowed_by_in_scope_generic() {
    use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::ParamInfo;
    use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::resolve::{
        resolve_type, ResolveContext,
    };
    use crate::adapters::shared::use_tree::ScopedAliasMap;

    let ty: syn::Type = syn::parse_str("::Q").expect("parse `::Q`");
    // Fixture invariant: `::Q` must carry leading_colon (else the test is vacuous).
    assert!(matches!(&ty, syn::Type::Path(tp) if tp.path.leading_colon.is_some()));
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
            reexports: None,
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

// (label, index files, use-site src, file path, fn index, extra crate-root
// files, build item-generics?, forbidden edge)
type LeadingColonCase = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static str,
    &'static str,
    usize,
    &'static [(&'static str, &'static str)],
    bool,
    &'static str,
);

const HANDLER: &str = "pub trait Handler { fn handle(&self); }";

const LEADING_COLON_CASES: &[LeadingColonCase] = &[
    (
        // `::Q::handle(&q)` inside `fn run<Q: Handler>()` — the leading `::`
        // disambiguates away from the in-scope generic param Q, so it must
        // not collapse to the Handler trait anchor.
        "leading-colon call shadowed by an in-scope generic param",
        &[("src/ports/handler.rs", HANDLER)],
        "use crate::ports::handler::Handler;\npub fn run<Q: Handler>(q: Q) { ::Q::handle(&q); }",
        "src/app/runner.rs",
        1,
        &[],
        true,
        "crate::ports::handler::Handler::handle",
    ),
    (
        // `::Q::handle()` with a workspace-local `Q` present — the leading
        // colon disambiguates away from the workspace symbol too.
        "leading-colon call not routed to a same-named workspace fn",
        &[(
            "src/app/q.rs",
            "pub struct Q;\nimpl Q { pub fn handle(&self) {} }",
        )],
        "pub struct Q;\nimpl Q { pub fn handle(&self) {} }\npub fn use_it() { ::Q::handle(); }",
        "src/cli/use_site.rs",
        2,
        &[],
        false,
        "crate::cli::use_site::Q::handle",
    ),
    (
        // `Q: ::ports::handler::Handler` — the bound's leading colon must be
        // preserved so `Q::handle()` does not emit the workspace anchor.
        "leading-colon generic bound not routed to the workspace trait",
        &[("src/ports/handler.rs", HANDLER)],
        "pub fn run<Q: ::ports::handler::Handler>(q: Q) { Q::handle(&q); }",
        "src/app/runner.rs",
        0,
        &[("src/ports/handler.rs", HANDLER)],
        true,
        "crate::ports::handler::Handler::handle",
    ),
    (
        // `&dyn ::ports::handler::Handler` — the dyn trait object's absolute
        // path must not route `x.handle()` through the workspace anchor.
        "leading-colon dyn-trait bound not routed to the workspace trait",
        &[("src/ports/handler.rs", HANDLER)],
        "pub fn use_it(x: &dyn ::ports::handler::Handler) { x.handle(); }",
        "src/app/use_site.rs",
        0,
        &[("src/ports/handler.rs", HANDLER)],
        false,
        "crate::ports::handler::Handler::handle",
    ),
];

#[test]
fn leading_colon_paths_do_not_route_to_workspace_anchors() {
    // An explicit leading `::` is the caller's disambiguation AWAY from both
    // in-scope generic params AND same-named workspace symbols (Rust 2018+:
    // `::X` is rooted at an extern crate). The call collector must therefore
    // never canonicalise these to a workspace trait-anchor or fn edge. Each
    // case drives the FileScope → FnContext → collect_canonical_calls pipeline
    // (via `calls_from_scoped`) and asserts the `forbidden` edge is absent.
    for (label, index_files, use_site, path, fn_index, extra_roots, with_generics, forbidden) in
        LEADING_COLON_CASES
    {
        let roots: Vec<&str> = index_files.iter().map(|(p, _)| *p).collect();
        let workspace_index = index_for(index_files, &roots);
        let calls = calls_from_scoped(ScopedCalls {
            workspace_index: &workspace_index,
            use_site_src: use_site,
            path,
            fn_index: *fn_index,
            extra_root_files: extra_roots,
            with_generics: *with_generics,
        });
        assert!(
            !calls.contains(*forbidden),
            "case {label}: `{forbidden}` must be absent (leading `::` \
             disambiguates away). Got calls: {calls:?}"
        );
    }
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
            reexports: None,
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
