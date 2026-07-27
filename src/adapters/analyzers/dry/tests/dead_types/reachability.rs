//! What keeps a declaration alive: who refers to it, and whether
//! anything reaches *them*. Cycles live here — the case a flat "is this
//! name mentioned" set cannot decide.

use std::collections::HashSet;

use super::{detect, detect_with, markers, names, Markers};
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
