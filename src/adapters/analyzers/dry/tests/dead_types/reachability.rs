//! What keeps a declaration alive: who refers to it, and whether
//! anything reaches *them*. Cycles live here — the case a flat "is this
//! name mentioned" set cannot decide.

use std::collections::HashSet;

use super::{detect, names, Markers};
use crate::adapters::analyzers::dry::dead_types::*;

#[test]
fn a_type_that_only_carries_its_own_methods_is_reported() {
    // rustc reaches the same verdict ("never constructed"); a trait impl does
    // not keep a type alive either.
    let found = detect("struct Lonely; impl Lonely { fn m(&self) {} }");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].name, "Lonely");
}

#[test]
fn a_self_referencing_type_does_not_keep_itself_alive() {
    // `struct Node { next: Option<Box<Node>> }` named itself in its own body,
    // which counted as a use — so a linked list nobody builds stayed invisible.
    // rustc calls the same type never constructed.
    let found = detect("struct Node { next: Option<Box<Node>> }");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].name, "Node");
}

#[test]
fn a_referenced_recursive_type_is_still_alive() {
    // The counterpart: suppressing the self-reference must not blind the check
    // to a real user elsewhere.
    assert!(
        names("struct Node { next: Option<Box<Node>> }\nfn f(n: Node) { let _ = n; }").is_empty()
    );
}

#[test]
fn two_types_that_only_refer_to_each_other_are_both_reported() {
    // The self-reference case one step out: each name occurs, so a flat "is it
    // mentioned anywhere" set finds both in use and reports nothing. Neither is
    // reachable from code that is not itself a candidate.
    let found = names("struct A { b: B }\nstruct B { a: Option<Box<A>> }");
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_cycle_of_three_types_is_reported() {
    // Cycle length is not a parameter: reachability marks what the roots reach,
    // so a ring of any size that no root enters stays unmarked.
    let found = names("struct A { b: B }\nstruct B { c: C }\nstruct C { a: Option<Box<A>> }");
    assert_eq!(found.len(), 3, "{found:?}");
}

#[test]
fn a_cycle_entered_from_production_stays_alive() {
    // The direction that matters: one entry point keeps the whole ring alive.
    let code = "struct A { b: B }\nstruct B { a: Option<Box<A>> }\nfn f(a: A) { let _ = a; }";
    assert!(names(code).is_empty(), "{:?}", detect(code));
}

#[test]
fn a_cycle_through_impl_methods_is_reported() {
    // How mutual references usually look in real code — not fields pointing at
    // each other, but methods converting between the two types. References made
    // inside an `impl` belong to the type the impl is for: if that type is dead,
    // so is everything only its methods name.
    let found = names(
        "struct A; struct B;\nimpl A { fn to_b(&self) -> B { B } }\n\
         impl B { fn to_a(&self) -> A { A } }",
    );
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_cycle_through_trait_impls_is_reported() {
    // How the mutual-conversion shape is usually spelled. It takes a different
    // branch of the impl walk than an inherent impl does.
    let found = names(
        "struct A; struct B;\nimpl From<A> for B { fn from(a: A) -> B { B } }\n\
         impl From<B> for A { fn from(b: B) -> A { A } }",
    );
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_cycle_across_files_is_reported() {
    // The owner map is keyed workspace-wide by bare name, and the value of
    // DRY-006 over rustc's own lint is exactly that it crosses file and crate
    // boundaries. A cycle that only holds together across two files must be
    // seen as one.
    let a = "pub struct A { b: crate::b::B }";
    let b = "pub struct B { a: Option<Box<crate::a::A>> }";
    let parsed = vec![
        (
            "src/a.rs".to_string(),
            a.to_string(),
            syn::parse_file(a).expect("parse"),
        ),
        (
            "src/b.rs".to_string(),
            b.to_string(),
            syn::parse_file(b).expect("parse"),
        ),
    ];
    let found = detect_dead_types(&parsed, &Markers::new(), &Markers::new(), &HashSet::new());
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_cycle_reached_only_from_tests_is_test_only() {
    // What a test-only entry point keeps alive is test-only too, all the way
    // down — reporting the reachable-from-tests part as "never used" would tell
    // the author to delete something the suite compiles against.
    let found = detect(
        "pub struct A { b: B }\npub struct B;\n#[cfg(test)]\n\
         mod tests { use super::A; fn t(a: A) { let _ = a; } }",
    );
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found.iter().all(|w| w.kind == DeadTypeKind::TestOnly),
        "{found:?}"
    );
}

#[test]
fn a_cfg_test_member_of_a_live_type_yields_a_test_only_reference() {
    // A production-live owner hands its `#[cfg(test)]` references on as *test*
    // references. Handing them on as production hides the finding; dropping
    // them reports "never used" for something the suite compiles against.
    let found = detect(
        "pub struct Fixture;\npub struct Prod;\n\
         impl Prod { #[cfg(test)] fn helper(&self) -> Fixture { Fixture } }\n\
         fn main() { let _ = Prod; }",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].name, "Fixture");
    assert_eq!(found[0].kind, DeadTypeKind::TestOnly);
}

#[test]
fn an_impl_on_a_type_from_outside_the_workspace_roots_its_references() {
    // Extension traits on foreign types are ordinary Rust. The self type is not
    // a declaration this check judges, so nothing could ever mark it live —
    // attributing the body to it would report everything the impl names as
    // dead, which is the expensive mistake, not a missed finding.
    let code = "trait Ext { fn go(&self); }\nstruct Helper;\n\
                impl Ext for Vec<u8> { fn go(&self) { let _ = Helper; } }";
    assert!(names(code).is_empty(), "{:?}", detect(code));
}

#[test]
fn a_blanket_impl_roots_its_references() {
    // The self type is a generic parameter, so no declaration owns the body.
    let code = "trait Ext { fn go(&self); }\nstruct Helper;\n\
                impl<T> Ext for T { fn go(&self) { let _ = Helper; } }";
    assert!(names(code).is_empty(), "{:?}", detect(code));
}

#[test]
fn a_blanket_impl_parameter_shadowing_a_real_type_still_roots() {
    // The case the previous test does *not* reach: `T` is not a declared name
    // either way, so it is rooted for being unknown. A parameter named like a
    // real declaration is only rooted because the impl's own generics are
    // consulted — without that, everything this body names would hang on a
    // verdict about the wrong `Item` and be reported as deletable.
    let code = "struct Item;\ntrait Ext { fn go(&self); }\nstruct Helper;\n\
                impl<Item> Ext for Item { fn go(&self) { let _ = Helper; } }";
    let found = names(code);
    assert!(!found.contains(&"Helper".to_string()), "{found:?}");
}

#[test]
fn a_qualified_impl_self_type_still_owns_its_body() {
    // Real code writes `impl crate::domain::Config`, not `impl Config`. Only
    // the last segment names the type, so the owner has to be read from there
    // or a whole codebase's impls would be rooted and find nothing.
    let found = names(
        "struct Config; struct Helper;\n\
         impl crate::Config { fn m(&self) { let _ = Helper; } }",
    );
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn a_type_named_only_by_a_trait_is_alive() {
    // A trait is not a candidate — DRY-006 deliberately does not judge traits —
    // so what one names is rooted, not owned. Treating a trait as an owner
    // would make every type in a trait signature depend on a verdict the check
    // never reaches.
    assert!(names("struct Payload; trait Sink { fn take(&self, p: Payload); }").is_empty());
}

#[test]
fn an_impl_on_a_foreign_type_is_not_owned_by_a_colliding_local_name() {
    // `impl Ext for foreign::Entry` says nothing about the local `Entry`.
    // Reading only the last path segment tied the body to a dead local
    // declaration, so a plainly used type was reported as deletable. The same
    // conflation the SRP owner key already refuses to make for absolute paths.
    let code = "struct Entry { n: u32 }\npub trait Ext { fn go(&self); }\npub struct Helper;\n\
                impl Ext for foreign::Entry { fn go(&self) { let _ = Helper; } }";
    let found = names(code);
    assert!(!found.contains(&"Helper".to_string()), "{found:?}");
}

#[test]
fn an_impl_reached_through_the_crate_root_is_still_owned() {
    // The counterpart: `crate::`, `self::` and `super::` name this crate, so
    // the last segment really is the local declaration. Rooting those too would
    // give up on how a large part of any codebase writes its impls.
    let found = names(
        "struct Config; struct Helper;\n\
         impl crate::Config { fn m(&self) { let _ = Helper; } }",
    );
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn an_impl_on_a_type_alias_does_not_own_the_body() {
    // An inherent impl on an alias attaches to the aliased type; whether the
    // alias itself is used is a different question, and answering the first
    // with the second reported what the methods name as dead.
    let code = "pub struct Foo;\ntype Alias = Foo;\npub struct Helper;\n\
                impl Alias { pub fn helper(&self) -> Helper { Helper } }\n\
                pub fn use_foo() -> Foo { Foo }";
    let found = names(code);
    assert!(!found.contains(&"Helper".to_string()), "{found:?}");
}

#[test]
fn an_unconsumed_variant_import_keeps_nothing_alive() {
    // `use Shape::*` is exposure like any other `use`. Recording its prefix
    // as a reference kept the enum alive with no variant ever used.
    let found =
        names("mod inner { pub enum Shape { Circle, Square } }\nuse inner::Shape::*;\nfn f() {}");
    assert_eq!(found, vec!["Shape"]);
}

#[test]
fn a_consumed_variant_keeps_its_enum_alive() {
    let cases = [
        "mod inner { pub enum Shape { Circle, Square } }\nuse inner::Shape::*;\nfn f() -> u8 { let _ = Circle; 1 }",
        "mod inner { pub enum Shape { Circle, Square } }\nuse inner::Shape::{Circle, Square};\nfn f() { let _ = (Circle, Square); }",
        "mod inner { pub enum Shape { Circle, Square } }\nuse inner::Shape::Circle as C;\nfn f() { let _ = C; }",
        // `Enum::{self as X}` renames the enum, not a module.
        "mod inner { pub enum Shape { Circle, Square } }\nuse inner::Shape::{self as Form};\nfn f() { let _ = Form::Circle; }",
        // A glob or a member through a renamed enum: the parent of the leaf is
        // the alias, and the enum lookup has to see through it.
        "mod inner { pub enum Shape { Circle, Square } }\nuse inner::Shape as Form;\nuse Form::*;\nfn f() { let _ = Circle; }",
        "mod inner { pub enum Shape { Circle, Square } }\nuse inner::Shape as Form;\nuse Form::Circle;\nfn f() { let _ = Circle; }",
        // The alias resolves transitively, and composes with a renamed member.
        "mod inner { pub enum Shape { Circle, Square } }\nuse inner::Shape as A;\nuse A as B;\nuse B::*;\nfn f() { let _ = Circle; }",
        "mod inner { pub enum Shape { Circle, Square } }\nuse inner::Shape as Form;\nuse Form::Circle as C;\nfn f() { let _ = C; }",
    ];
    for code in cases {
        assert!(names(code).is_empty(), "{code}");
    }
}

#[test]
fn a_module_segment_in_a_use_does_not_vouch_for_a_same_named_type() {
    // The `Shape` in the path is a module; the struct `Shape` has no user. A
    // prefix recorded as a bare name conflated the two and hid the finding.
    let found = names(
        "struct Shape;\nmod inner { pub mod Shape { pub fn foo() {} } }\nuse inner::Shape::foo;\nfn main() { foo(); }",
    );
    assert_eq!(found, vec!["Shape"]);
}

#[test]
fn a_same_named_variant_of_another_enum_does_not_vouch_for_an_unused_one() {
    // `Kind::Other` is declared and used; `Shape::Other` exists too and only
    // the import names `Shape`. A variant's own declaration is not a
    // reference, so the implication (Other → Shape) must not fire off `Kind`'s
    // declaration of its own `Other`.
    let found = names(
        "mod inner { pub enum Shape { Circle, Other } }\nuse inner::Shape::*;\n\
         pub enum Kind { Some, Other }\nfn f(_: Kind) {}",
    );
    assert_eq!(found, vec!["Shape"]);
}

#[test]
fn an_unconsumed_glob_through_a_renamed_enum_keeps_nothing_alive() {
    // The alias is resolved for the lookup, not counted as consumption: with
    // no variant used, the renamed enum is as dead as a plainly named one.
    let found = names(
        "mod inner { pub enum Shape { Circle, Square } }\nuse inner::Shape as Form;\nuse Form::*;\nfn f() {}",
    );
    assert_eq!(found, vec!["Shape"]);
}

#[test]
fn a_variant_reached_through_a_test_alias_is_test_only() {
    // Whichever context declares the alias, resolving it for the lookup must
    // not move the reference out of the context that made it: the enum is
    // test-only, never unused, and never production-used.
    let cases = [
        "mod inner { pub enum Shape { Circle } }\n#[cfg(test)]\nmod t { use super::inner::Shape as Form; use Form::*; fn t() { let _ = Circle; } }",
        "mod inner { pub enum Shape { Circle } }\nuse inner::Shape as Form;\n#[cfg(test)]\nmod t { use super::Form::*; fn t() { let _ = Circle; } }",
    ];
    for code in cases {
        let found = detect(code);
        let kinds: Vec<_> = found.iter().map(|w| (w.name.as_str(), w.kind)).collect();
        assert_eq!(kinds, vec![("Shape", DeadTypeKind::TestOnly)], "{code}");
    }
}

#[test]
fn a_variant_glob_resolves_across_files_in_either_order() {
    // The enum map is built over the whole walk and consulted afterwards, so
    // the file declaring the enum may come before or after the one importing
    // its variants.
    let root = "mod shapes;\nuse shapes::Shape::*;\npub fn f() -> u8 { let _ = Circle; 1 }";
    let shapes = "pub enum Shape { Circle, Square }";
    for files in [
        [("src/lib.rs", root), ("src/shapes.rs", shapes)],
        [("src/shapes.rs", shapes), ("src/lib.rs", root)],
    ] {
        let parsed: Vec<_> = files
            .iter()
            .map(|(p, c)| (p.to_string(), c.to_string(), syn::parse_file(c).unwrap()))
            .collect();
        let found = detect_dead_types(&parsed, &Markers::new(), &Markers::new(), &HashSet::new());
        assert!(found.is_empty(), "{files:?}: {found:?}");
    }
}

#[test]
fn a_test_alias_does_not_resolve_a_production_import() {
    // Production has its own `Form`; only the test module says `Shape as
    // Form`. The production glob is resolved with production renames alone,
    // so it reaches `Form`, and `Shape` stays what rustc says it is: unused.
    let found = names(
        "pub enum Shape { Circle }\npub enum Form { Circle }\nuse Form::*;\nfn f() { let _ = Circle; }\n\
         #[cfg(test)]\nmod t { use super::Shape as Form; }",
    );
    assert_eq!(found, vec!["Shape"]);
}

#[test]
fn a_module_and_an_enum_sharing_name_and_member_pool_known_limit() {
    // `inner::Shape` is a module with a const `Red`; the enum `Shape { Red }`
    // is dead. By bare name the two are one, and the import's implication
    // (Red → Shape) keeps the enum alive. Telling them apart needs a resolver;
    // the limit errs toward a missed finding.
    let found = names(
        "pub enum Shape { Red }\nmod inner { pub mod Shape { pub const Red: u8 = 1; } }\n\
         use inner::Shape::Red;\nfn f() -> u8 { Red }",
    );
    assert!(found.is_empty(), "{found:?}");
}

/// A binding made by a `use` in module `m` is reached as `m::name` from
/// anywhere — through the path's *last* segment. Each shape here has to keep
/// the declaration behind the binding alive.
const CONSUMED_THROUGH_A_QUALIFIED_BINDING: &[(&str, &str)] = &[
    (
        "renamed type through a facade module",
        "mod inner { pub struct Real; }\nmod facade { pub use crate::inner::Real as Facade; }\nfn takes(_: facade::Facade) {}",
    ),
    (
        "renamed type through an alias of the crate root",
        "mod inner { pub struct Real; }\nuse crate as API;\npub(crate) use inner::Real as Alias;\nfn takes(_: API::Alias) {}",
    ),
    (
        "renamed type through an extern crate self alias",
        "mod inner { pub struct Real; }\nextern crate self as API;\npub(crate) use inner::Real as Alias;\nfn takes(_: API::Alias) {}",
    ),
    // An external crate is a module by definition, whether it is named by
    // `extern crate` or only by a `use` of it; a CamelCase alias of one is a
    // module-qualified path. (The re-export lives in this crate here; in the
    // real two-crate shape it lives in the aliased one, same grain.)
    (
        "renamed type through an alias of an extern crate",
        "mod inner { pub struct Real; }\nextern crate dep as API;\npub(crate) use inner::Real as Alias;\nfn takes(_: API::Alias) {}",
    ),
    (
        "renamed type through a re-export a macro_rules! transcriber generates",
        "mod inner { pub struct Real; }\n\
         macro_rules! expose { () => { mod api { pub use crate::inner::Real as Alias; } }; }\nexpose!();\n\
         fn takes(_: api::Alias) {}",
    ),
    (
        "renamed type through a use alias of an external crate",
        "mod inner { pub struct Real; }\nuse dep as API;\npub(crate) use inner::Real as Alias;\nfn takes(_: API::Alias) {}",
    ),
    (
        "renamed type through the crate root",
        "mod inner { pub struct Other; }\npub use inner::Other as TopAlias;\nmod m { fn takes(_: crate::TopAlias) {} }",
    ),
    (
        "variant glob re-exported by a facade module",
        "mod inner { pub enum Shape { Circle } }\nmod facade { pub use crate::inner::Shape::*; }\nfn f() { let _ = facade::Circle; }",
    ),
    (
        "renamed const used as a pattern",
        "mod inner { pub const LIMIT: u8 = 3; }\nuse inner::LIMIT as MAX;\nfn classify(n: u8) -> u8 { match n { MAX => 1, _ => 0 } }",
    ),
    (
        "glob-imported variant used only as a pattern",
        "mod inner { pub enum Shape { Circle } }\nuse inner::Shape::*;\nfn f(s: Option<u8>) -> u8 { match s { Some(Circle) => 1, _ => 0 } }",
    ),
    (
        "struct-like variant constructed and matched after a glob",
        "mod inner { pub enum Shape { Circle { r: u8 } } }\nuse inner::Shape::*;\nfn f() -> u8 { match (Circle { r: 1 }) { Circle { r } => r } }",
    ),
    (
        "tuple variant called through an enum alias",
        "mod inner { pub enum Shape { Circle(u8) } }\nuse inner::Shape as Form;\nfn f() -> Form { Form::Circle(1) }",
    ),
];

#[test]
fn a_declaration_consumed_through_a_qualified_binding_is_alive() {
    for (label, code) in CONSUMED_THROUGH_A_QUALIFIED_BINDING {
        let found = names(code);
        assert!(found.is_empty(), "{label}: {found:?}");
    }
}

#[test]
fn a_self_qualified_variant_is_a_use_of_the_impl_owner() {
    // `Self::Circle` inside `impl Kind` is `Kind::Circle`: a use of `Kind`,
    // not of the `Circle` that `use Shape::*` brought in.
    let found = names(
        "pub enum Shape { Circle }\npub enum Kind { Circle }\nuse Shape::*;\n\
         impl Kind { fn make() -> Kind { Self::Circle } }\nfn f() -> Kind { Kind::make() }",
    );
    assert_eq!(found, vec!["Shape"]);
}

/// The last segment of a qualified path is a binding only when the qualifier
/// can be a module. A CamelCase qualifier names a type — a declared enum, an
/// alias of one, one from another crate — and its last segment is a variant or
/// an associated item, never something a `use` bound.
const QUALIFIED_BY_A_TYPE: &[(&str, &str)] = &[
    (
        "a declared enum",
        "pub enum Shape { Circle }\npub enum Kind { Circle }\nuse Shape::*;\nfn f() { let _ = Kind::Circle; }",
    ),
    (
        "an enum from another crate",
        "pub enum Shape { Less }\nuse Shape::*;\nfn f() -> std::cmp::Ordering { std::cmp::Ordering::Less }",
    ),
];

#[test]
fn a_variant_qualified_by_a_type_is_not_a_use_of_a_glob_import() {
    for (label, code) in QUALIFIED_BY_A_TYPE {
        assert_eq!(names(code), vec!["Shape"], "{label}");
    }
}

#[test]
fn a_declared_module_named_like_a_type_still_carries_its_bindings() {
    // `Facade` is CamelCase, and an unrelated enum `Facade { Alias }` exists
    // — yet at the use site `Facade` is a *module* re-exporting `Real`.
    // Deciding by the enum alone reported `Real` dead; a declared module
    // name overrides the convention, and the ambiguity errs toward alive —
    // for the enum too, which the path's first segment names by bare name.
    let found = names(
        "mod other { pub enum Facade { Alias } }\nmod inner { pub struct Real; }\n\
         #[allow(non_snake_case)]\nmod Facade { pub use crate::inner::Real as Alias; }\n\
         fn takes(_: Facade::Alias) {}",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_camel_case_alias_of_a_module_still_carries_its_bindings() {
    // `use shapes as Shapes` names a module however it is spelled, so
    // `Shapes::Circle` reaches the glob re-export inside it.
    let found = names(
        "mod shapes { pub enum Shape { Circle, Square } pub use self::Shape::*; }\n\
         use shapes as Shapes;\nfn f() { let _ = [Shapes::Circle, Shapes::Square]; }",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_variant_qualified_by_an_enum_alias_pools_known_limit() {
    // `use Other as Kind; Kind::Circle` is a use of `Other`. But `use Other
    // as Kind` reads the same when `Other` is an external crate, so the alias
    // counts as a module, `Circle` is a head, and the glob keeps `Shape`
    // alive. The limit errs toward a missed finding.
    let found = names(
        "pub enum Shape { Circle }\npub enum Other { Circle }\nuse Other as Kind;\nuse Shape::*;\n\
         fn f() -> Other { Kind::Circle }",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_crate_shadowed_by_a_same_named_type_elsewhere_still_carries_its_bindings() {
    // `use Dep as API` at the crate root sees the crate `Dep`; the struct in
    // `unrelated` is out of scope there. Reading every declared type name as
    // "not a module" reported `Real` dead. The struct itself pools with the
    // crate by bare name and stays alive too — the contract's direction.
    let found = names(
        "mod unrelated { pub struct Dep; }\nmod inner { pub struct Real; }\n\
         use Dep as API;\npub(crate) use inner::Real as Alias;\nfn takes(_: API::Alias) {}",
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_metavariable_is_not_a_reference_to_a_same_named_type() {
    // `$T` in a transcriber is whatever the invocation passes, not the
    // struct `T`.
    let found = names(
        "pub struct T;\nmacro_rules! hold { ($T:ty) => { let _: Option<$T> = None; }; }\nfn f() {}",
    );
    assert_eq!(found, vec!["T"]);
}

#[test]
fn a_doc_comment_on_a_use_still_counts() {
    // A `use` records no name, but its attributes are attributes like any
    // other: an intra-doc link documents the API, a doc-test fence is code
    // `cargo test` runs. Skipping them with the rest of the `use` lost both.
    let found = names(
        "pub struct Linked;\npub struct Tested;\nmod inner { pub fn f() {} }\n\
         /// See [`Linked`].\n///\n/// ```\n/// let _ = Tested;\n/// ```\npub use inner::f;",
    );
    assert_eq!(found, vec!["Tested"], "the doc-test use is test-only");
}
