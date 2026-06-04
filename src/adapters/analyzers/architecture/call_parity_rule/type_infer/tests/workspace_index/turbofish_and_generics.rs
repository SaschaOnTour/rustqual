use super::*;

// Shared fixture files for the turbofish-substitution rule table: a Handler
// trait (port) and a Session with an inherent `diff` (app). Each case adds a
// third file declaring the generic-returning fn/method under test.
const TF_HANDLER: (&str, &str) = (
    "src/ports/handler.rs",
    "pub trait Handler { fn handle(&self); }",
);
const TF_SESSION: (&str, &str) = (
    "src/app/session.rs",
    "pub struct Session;\nimpl Session { pub fn diff(&self) {} }",
);

// (label, declarer-file (path, src), use-site fn body, session_diff_expected)
type TurbofishCase = (
    &'static str,
    (&'static str, &'static str),
    &'static str,
    bool,
);

const TURBOFISH_CASES: &[TurbofishCase] = &[
        (
            // free fn `get<Q: Handler>() -> Q`: turbofish overrides the
            // bounded-generic return; `.diff()` → Session::diff, not Handler.
            "free-fn bounded generic param return",
            (
                "src/app/make.rs",
                "use crate::ports::handler::Handler;\npub fn get<Q: Handler>() -> Q { unimplemented!() }",
            ),
            "use crate::app::make::get;\nuse crate::app::session::Session;\npub fn use_it() { get::<Session>().diff(); }",
            true,
        ),
        (
            // method `current<Q: Handler>(&self) -> Q`: same override, method
            // call. `s: &Service` typed via the sig param so the receiver
            // binds as Path([Service]).
            "method-call bounded generic param return",
            (
                "src/app/service.rs",
                "use crate::ports::handler::Handler;\npub struct Service;\nimpl Service { pub fn current<Q: Handler>(&self) -> Q { unimplemented!() } }",
            ),
            "use crate::app::service::Service;\nuse crate::app::session::Session;\npub fn use_it(s: &Service) { s.current::<Session>().diff(); }",
            true,
        ),
        (
            // free fn `make<T>() -> impl Handler`: opaque return; T does NOT
            // substitute it, so no Session::diff edge (TraitBound, not
            // GenericParamBound).
            "free-fn impl-Trait return is opaque",
            (
                "src/app/make.rs",
                "use crate::ports::handler::Handler;\npub fn make<T>() -> impl Handler { unimplemented!() }",
            ),
            "use crate::app::make::make;\nuse crate::app::session::Session;\npub fn use_it() { make::<Session>().diff(); }",
            false,
        ),
        (
            // method `make_handler<T>(&self) -> impl Handler`: opaque, same.
            "method-call impl-Trait return is opaque",
            (
                "src/app/service.rs",
                "use crate::ports::handler::Handler;\npub struct Service;\nimpl Service { pub fn make_handler<T>(&self) -> impl Handler { unimplemented!() } }",
            ),
            "use crate::app::service::Service;\nuse crate::app::session::Session;\npub fn use_it() { let s = Service; s.make_handler::<Session>().diff(); }",
            false,
        ),
        (
            // free fn `get<Q: Handler>() -> Result<Q, MyErr>`: substitution
            // recurses into the Result wrapper; `.unwrap().diff()` → Session.
            "free-fn Result-wrapped generic param return",
            (
                "src/app/make.rs",
                "use crate::ports::handler::Handler;\npub struct MyErr;\npub fn get<Q: Handler>() -> Result<Q, MyErr> { unimplemented!() }",
            ),
            "use crate::app::make::get;\nuse crate::app::session::Session;\npub fn use_it() { get::<Session>().unwrap().diff(); }",
            true,
        ),
        (
            // method `current<Q: Handler>(&self) -> Result<Q, MyErr>`: same
            // wrapper recursion, method call.
            "method-call Result-wrapped generic param return",
            (
                "src/app/service.rs",
                "use crate::ports::handler::Handler;\npub struct MyErr;\npub struct Service;\nimpl Service { pub fn current<Q: Handler>(&self) -> Result<Q, MyErr> { unimplemented!() } }",
            ),
            "use crate::app::service::Service;\nuse crate::app::session::Session;\npub fn use_it(s: &Service) { s.current::<Session>().unwrap().diff(); }",
            true,
        ),
        (
            // free fn `get<Q: Handler>() -> Vec<Q>`: substitution recurses into
            // the Slice; `for s in get::<Session>() { s.diff(); }` → Session.
            "free-fn Vec-wrapped generic param return (iterated)",
            (
                "src/app/make.rs",
                "use crate::ports::handler::Handler;\npub fn get<Q: Handler>() -> Vec<Q> { unimplemented!() }",
            ),
            "use crate::app::make::get;\nuse crate::app::session::Session;\npub fn use_it() { for s in get::<Session>() { s.diff(); } }",
            true,
        ),
];

#[test]
fn turbofish_substitution_through_generic_param_returns() {
    // A turbofish `::<Session>` substitutes a bare generic-param return
    // (`-> Q`, including inside Result/Vec wrappers) so a trailing `.diff()`
    // resolves to the concrete `Session::diff` — but an `impl Trait` return is
    // opaque and must NOT substitute. Each case drives the full inference
    // pipeline via `calls_from`; `expected` says whether the `Session::diff`
    // edge must appear. (Behaviour lives in `collect_canonical_calls`; the
    // index alone can't surface it.)
    const SESSION_DIFF: &str = "crate::app::session::Session::diff";
    for (label, declarer, use_site, expected) in TURBOFISH_CASES {
        let files = [TF_HANDLER, TF_SESSION, *declarer];
        let roots: Vec<&str> = files.iter().map(|(p, _)| *p).collect();
        let workspace_index = index_for(&files, &roots);
        let calls = calls_from(&workspace_index, use_site, 2);
        assert_eq!(
            calls.contains(SESSION_DIFF),
            *expected,
            "case {label}: Session::diff edge presence mismatch. Calls: {calls:?}"
        );
    }
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
    // Strict: the method's where-clause bound on the impl-level generic must
    // be captured by `method_canonical_generics`; if the extractor drops the
    // outer-name predicate the param has empty bounds and the return is
    // Opaque. Capturing it surfaces a canonical `GenericParamBound` (with
    // `turbofish_index: None`, since impl-level Q substitutes via the receiver
    // type, not a method-call turbofish on `current`).
    let index = index_for(
        &[
            (
                "src/ports/handler.rs",
                "pub trait Handler { fn handle(&self); }",
            ),
            (
                "src/app/service.rs",
                "use crate::ports::handler::Handler;\npub struct Service<Q>(pub Q);\nimpl<Q> Service<Q> { pub fn current(&self) -> Q where Q: Handler { unimplemented!() } }",
            ),
        ],
        &["src/ports/handler.rs", "src/app/service.rs"],
    );
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
    let index = index_for(
        &[(
            "src/app/make.rs",
            "pub struct Q;\npub struct Container<Q> { pub item: Q }",
        )],
        &["src/app/make.rs"],
    );
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
    let index = index_for(
        &[(
            "src/app/make.rs",
            "pub struct Q;\npub struct Service<Q>(pub Q);\nimpl<Q> Service<Q> { pub fn first(&self) -> Q { unimplemented!() } }",
        )],
        &["src/app/make.rs"],
    );
    assert_eq!(
        index.method_return("crate::app::make::Service", "first"),
        None,
        "impl-level generic `Q` must shadow workspace struct `Q` for \
         every method's return resolution. Got: {:?}",
        index.method_returns,
    );
}
