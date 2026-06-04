use super::*;

// ── method_returns ───────────────────────────────────────────────

/// A method-return indexing case: `(label, files, roots, receiver, method,
/// expect)`. `expect = Some(ty)` asserts `method_return(receiver, method)`
/// equals that exact type; `None` asserts the method is not indexed.
/// A tuple (not a struct) keeps the data table clear of struct-construction
/// boilerplate (BP-009). `CanonicalType` is built at runtime, hence `vec!`.
type MethodCase = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static [&'static str],
    &'static str,
    &'static str,
    Option<CanonicalType>,
);

/// Concrete-return cases: inherent, `Result<T, E>`, `Result<Self, E>`, and unit
/// returns. Split from the generic cases so each builder stays a small "arrange"
/// step and the test body reads as plain act+assert (no `qual:allow` needed).
fn method_return_concrete_cases() -> Vec<MethodCase> {
    vec![
        (
            "inherent method with concrete return",
            &[(
                "src/app/session.rs",
                "pub struct Session;\npub struct Response;\nimpl Session { pub fn diff(&self) -> Response { Response } }",
            )],
            &["src/app/session.rs"],
            "crate::app::session::Session",
            "diff",
            Some(CanonicalType::path(["crate", "app", "session", "Response"])),
        ),
        (
            "Result<T, E> return wraps T",
            &[(
                "src/app/session.rs",
                "pub struct Session;\npub struct Response;\npub struct Error;\nimpl Session { pub fn diff(&self) -> Result<Response, Error> { unimplemented!() } }",
            )],
            &["src/app/session.rs"],
            "crate::app::session::Session",
            "diff",
            Some(CanonicalType::Result(Box::new(CanonicalType::path([
                "crate", "app", "session", "Response",
            ])))),
        ),
        (
            // `Result<Self, _>` must store `Result<Session>`, not
            // `Result<Opaque>`, or chains like `open().unwrap().diff()`
            // lose the receiver type at `.unwrap()`.
            "Result<Self, E> substitutes the receiver type",
            &[(
                "src/app/session.rs",
                "pub struct Session;\npub struct Error;\nimpl Session { pub fn open() -> Result<Self, Error> { unimplemented!() } }",
            )],
            &["src/app/session.rs"],
            "crate::app::session::Session",
            "open",
            Some(CanonicalType::Result(Box::new(CanonicalType::path([
                "crate", "app", "session", "Session",
            ])))),
        ),
        (
            "unit return is not indexed",
            &[("src/app/foo.rs", "pub struct S;\nimpl S { pub fn bump(&self) {} }")],
            &["src/app/foo.rs"],
            "crate::app::foo::S",
            "bump",
            None,
        ),
    ]
}

/// Generic / trait-dispatch cases: impl-Trait return, trait-impl method keyed by
/// the receiver type, and a method-scoped generic shadowing a workspace type.
fn method_return_generic_cases() -> Vec<MethodCase> {
    vec![
        (
            "impl-Trait return is not indexed",
            &[(
                "src/app/foo.rs",
                "pub struct S;\nimpl S { pub fn iter(&self) -> impl Iterator<Item = u8> { std::iter::empty() } }",
            )],
            &["src/app/foo.rs"],
            "crate::app::foo::S",
            "iter",
            None,
        ),
        (
            // Keyed by the concrete receiver type S, NOT by the trait.
            "trait-impl method is keyed by the receiver type",
            &[(
                "src/app/foo.rs",
                "pub struct S;\npub struct T;\npub trait Convert { fn to(&self) -> T; }\nimpl Convert for S { fn to(&self) -> T { T } }",
            )],
            &["src/app/foo.rs"],
            "crate::app::foo::S",
            "to",
            Some(CanonicalType::path(["crate", "app", "foo", "T"])),
        ),
        (
            // method-scoped generic `Q` must shadow workspace `struct Q`,
            // yield Opaque, and not poison method_returns.
            "method-scoped generic param shadows a same-named workspace type",
            &[(
                "src/app/make.rs",
                "pub struct Q;\npub struct Service;\nimpl Service { pub fn get<Q>(&self) -> Q { unimplemented!() } }",
            )],
            &["src/app/make.rs"],
            "crate::app::make::Service",
            "get",
            None,
        ),
    ]
}

#[test]
fn method_return_indexing() {
    let mut cases = method_return_concrete_cases();
    cases.extend(method_return_generic_cases());
    for (label, files, roots, receiver, method, expect) in cases {
        let index = index_for(files, roots);
        assert_eq!(
            index.method_return(receiver, method),
            expect.as_ref(),
            "case: {label}"
        );
    }
}

// ── fn_returns ───────────────────────────────────────────────────

/// A free-fn return indexing case: `(label, files, roots, fn_canonical,
/// expect)`. `Some(ty)` asserts `fn_return(fn_canonical)` equals that type;
/// `None` asserts the fn is not indexed. Tuple (not struct) to avoid BP-009.
type FnCase = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static [&'static str],
    &'static str,
    Option<CanonicalType>,
);

#[test]
fn fn_return_indexing() {
    let cases: Vec<FnCase> = vec![
        (
            "concrete return is indexed",
            &[(
                "src/app/make.rs",
                "pub struct Session;\npub fn make_session() -> Session { Session }",
            )],
            &["src/app/make.rs"],
            "crate::app::make::make_session",
            Some(CanonicalType::path(["crate", "app", "make", "Session"])),
        ),
        (
            // Generic T has no alias/local-symbol entry → Opaque → skipped.
            "generic return type is opaque and not indexed",
            &[(
                "src/app/make.rs",
                "pub fn get<T>() -> T { unimplemented!() }",
            )],
            &["src/app/make.rs"],
            "crate::app::make::get",
            None,
        ),
        (
            // fn-scoped generic `Q` must shadow workspace `struct Q`, yield
            // Opaque, and not be indexed (else turbofish inference breaks).
            "fn-scoped generic param shadows a same-named workspace type",
            &[(
                "src/app/make.rs",
                "pub struct Q;\npub fn get<Q>() -> Q { unimplemented!() }",
            )],
            &["src/app/make.rs"],
            "crate::app::make::get",
            None,
        ),
    ];
    for (label, files, roots, fn_canonical, expect) in cases {
        let index = index_for(files, roots);
        assert_eq!(
            index.fn_return(fn_canonical),
            expect.as_ref(),
            "case: {label}"
        );
    }
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
    // Acceptable outcomes: skipped entirely (Opaque) or stored as
    // GenericParamBound with the canonical path. Forbidden: a bound carrying
    // the raw single-segment `["Handler"]`. (Bare generic-param returns use
    // `GenericParamBound`, not `TraitBound`.)
    let index = index_for(
        &[
            (
                "src/ports/handler.rs",
                "pub trait Handler { fn handle(&self); }",
            ),
            (
                "src/app/make.rs",
                "use crate::ports::handler::Handler;\npub fn make<Q: Handler>() -> Q { unimplemented!() }",
            ),
        ],
        &["src/ports/handler.rs", "src/app/make.rs"],
    );
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
