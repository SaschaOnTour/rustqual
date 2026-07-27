//! Which declarations are *reachable*, not merely mentioned.
//!
//! A flat "does this name occur anywhere" set answers the wrong question as soon
//! as declarations refer to each other. `struct A { b: B }` next to
//! `struct B { a: Option<Box<A>> }` mentions both names, so both look used —
//! while nothing outside the pair can reach either. The same holds for a ring of
//! three, of four, of any size, and for the common real shape where the mutual
//! references sit in `impl` methods rather than in fields.
//!
//! Cycle length is therefore not a parameter to pick. What is needed is one
//! attribution — *which declaration made this reference* — and then a walk from
//! the roots: whatever the walk does not mark is dead, however tangled it is
//! among itself. That is how rustc's own `dead_code` lint works, and it is why
//! this is a graph rather than another special case.
//!
//! **Roots** are references made from code that is not itself a candidate: a
//! function body, a trait definition, a `use`. Being `pub` is deliberately *not*
//! a root — a public type nobody in the workspace uses is exactly the finding,
//! and a library whose consumers live elsewhere says so with `// qual:api`.
//! Declarations that are exempt anyway (`#[allow(dead_code)]`, `qual:api`, a
//! leading `_`) are seeded as live, so what they name keeps its user.
//!
//! The two contexts stay apart, because "only tests use it" is a different
//! finding from "nothing uses it". A live production declaration passes its
//! `#[cfg(test)]` references on as test references; a declaration only tests
//! reach passes *everything* on as test references, since that is all its own
//! reachability amounts to.

use std::collections::{HashMap, HashSet};

use super::split_names::ContextRefs;

/// Who refers to what.
#[derive(Debug, Default)]
pub(crate) struct ReferenceGraph {
    /// References made from outside any candidate declaration. These start the
    /// walk.
    pub(crate) roots: ContextRefs,
    /// Declaration name → what its own body refers to. Keyed by bare name, the
    /// same grain as the reference set itself: two same-named declarations pool,
    /// which can only keep something alive, never kill it.
    pub(crate) owners: HashMap<String, ContextRefs>,
}

impl ReferenceGraph {
    /// Record that `owner` (or, for `None`, rooted code) refers to `name`.
    /// Integration: bucket lookup, then the insert.
    pub(crate) fn add(&mut self, owner: Option<&str>, in_test: bool, name: String) {
        self.bucket(owner).set(in_test).insert(name);
    }

    /// The pair of sets a reference from `owner` lands in.
    /// Operation: one branch over the owner, no own calls.
    fn bucket(&mut self, owner: Option<&str>) -> &mut ContextRefs {
        match owner {
            Some(name) => self.owners.entry(name.to_string()).or_default(),
            None => &mut self.roots,
        }
    }

    /// Every name the graph holds, ignoring who referred to it — except a
    /// declaration naming itself, which is nobody using it.
    ///
    /// The question "does production mention this name at all" — which is what
    /// verifying a `qual:api` marker asks — is not the reachability question,
    /// and answering it from the graph keeps one collection pass serving both.
    /// The self-edge has to go, or a `qual:api` on a self-referential type
    /// would read as spent: the marker is doing its job precisely because
    /// nothing *else* names the type.
    /// Operation: two filtered merges, own calls hidden in the closure.
    pub(crate) fn flatten(&self) -> ContextRefs {
        let mut all = ContextRefs {
            production: self.roots.production.clone(),
            tests: self.roots.tests.clone(),
        };
        self.owners.iter().for_each(|(owner, refs)| {
            let other = |set: &HashSet<String>| -> Vec<String> {
                set.iter().filter(|n| *n != owner).cloned().collect()
            };
            all.production.extend(other(&refs.production));
            all.tests.extend(other(&refs.tests));
        });
        all
    }
}

/// Mark everything the roots reach, in the context they reach it in.
///
/// Two kinds of owner hand their references on without having to be reached
/// themselves, and both do so by contributing their *edges*, not their own name
/// — a declaration that is excused from a finding is not thereby alive.
///
/// `candidates` are the declarations under judgement; an owner outside that set
/// is one of them. `impl Ext for Vec<u8>` attributes its body to a name this
/// check never decides, so nothing could mark it live and everything the impl
/// names would read as dead. Assuming such an owner alive costs a missed
/// finding; the other direction costs a wrong one.
///
/// `excused` are the declarations whose own liveness is not in question:
/// `#[allow(dead_code)]`, `// qual:api`, a leading `_`, and `// qual:test_helper`.
/// What they name has a real user, so it must not be reported — the same reason
/// DRY-002 counts a `qual:test_helper` function's calls as production calls.
/// Operation: worklist over the graph, own calls hidden in the closures.
pub(crate) fn resolve(
    graph: &ReferenceGraph,
    candidates: &HashSet<String>,
    excused: &HashSet<String>,
) -> ContextRefs {
    let follow = |name: &str, in_test: bool| followed(graph, name, in_test);
    let seeded = |name: &String| !candidates.contains(name) || excused.contains(name);
    let mut queue: Vec<(&str, bool)> = graph
        .roots
        .production
        .iter()
        .map(|n| (n.as_str(), false))
        .chain(graph.roots.tests.iter().map(|n| (n.as_str(), true)))
        .chain(
            graph
                .owners
                .keys()
                .filter(|n| seeded(n))
                .flat_map(|n| follow(n, false)),
        )
        .collect();
    let mut alive = ContextRefs::default();
    while let Some((name, in_test)) = queue.pop() {
        if !alive.set(in_test).insert(name.to_string()) {
            continue;
        }
        queue.extend(follow(name, in_test));
    }
    alive
}

/// What a declaration that is alive in `in_test` context keeps alive in turn.
/// A test-only declaration hands *all* of its references on as test references:
/// its production ones are only compiled because it is, and it is only there for
/// the tests.
/// Operation: map lookup + two mapped iterators, no own calls.
fn followed<'a>(graph: &'a ReferenceGraph, name: &str, in_test: bool) -> Vec<(&'a str, bool)> {
    graph
        .owners
        .get(name)
        .into_iter()
        .flat_map(|refs| {
            refs.production
                .iter()
                .map(move |n| (n.as_str(), in_test))
                .chain(refs.tests.iter().map(|n| (n.as_str(), true)))
        })
        .collect()
}
