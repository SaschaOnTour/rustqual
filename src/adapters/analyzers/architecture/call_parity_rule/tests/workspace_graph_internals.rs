//! Unit tests for `workspace_graph` internal predicates that the
//! integration-style graph tests don't pin tightly enough: the
//! crate-rooted impl-path branch in `canonical_fn_name`, the
//! anchor-backed-concrete capability conjunction, and the
//! inherited-default-match guard chain. Each isolates one boolean so
//! the matching mutant dies.

use crate::adapters::analyzers::architecture::call_parity_rule::anchor_index::AnchorInfo;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::workspace_index::WorkspaceTypeIndex;
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::edge_rewrite::is_inherited_default_match;
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::{
    canonical_fn_name, CallGraph,
};
use std::collections::{HashMap, HashSet};

// ── canonical_fn_name: crate-rooted impl path branch ────────────────────

#[test]
fn canonical_fn_name_prepends_file_module_for_relative_self_type() {
    // A non-crate-rooted self type (`Some(["Foo"])`) must be prefixed with
    // `crate` + the file's module segments. Pins `is_crate_rooted` against
    // `-> true` (which would take the as-is arm and drop the prefix).
    let got = canonical_fn_name(
        "src/application/x.rs",
        Some(&["Foo".to_string()]),
        &[],
        "method",
    );
    assert_eq!(got, "crate::application::x::Foo::method");
}

#[test]
fn canonical_fn_name_uses_crate_rooted_self_type_as_is() {
    // An already-crate-rooted self type is used verbatim — no file prefix.
    // The positive control for `is_crate_rooted` (this arm is identical
    // under the `-> true` mutant, so it pins the as-is path's correctness
    // rather than the mutant).
    let got = canonical_fn_name(
        "src/application/x.rs",
        Some(&["crate".to_string(), "foo".to_string(), "Bar".to_string()]),
        &[],
        "method",
    );
    assert_eq!(got, "crate::foo::Bar::method");
}

// ── is_anchor_backed_concrete: capability AND containment ────────────────

fn graph_with_anchor(anchor: &str, info: AnchorInfo) -> CallGraph {
    let mut anchors = HashMap::new();
    anchors.insert(anchor.to_string(), info);
    CallGraph {
        forward: HashMap::new(),
        reverse: HashMap::new(),
        layer_of: HashMap::new(),
        trait_method_anchors: anchors,
    }
}

/// A target-capable anchor (visible trait declared in `application`
/// with a default body) whose overriding-impl canonicals are `impls`.
fn target_capable_anchor(impls: &[&str]) -> AnchorInfo {
    AnchorInfo {
        impl_layers: HashSet::new(),
        impl_method_canonicals: impls.iter().map(|s| s.to_string()).collect(),
        decl_layer: Some("application".to_string()),
        has_default_body: true,
        trait_visible: true,
        location: None,
    }
}

#[test]
fn anchor_backed_concrete_requires_both_capability_and_containment() {
    let concrete = "crate::application::logging::LoggingHandler::handle";
    let adapters = vec!["cli".to_string(), "mcp".to_string()];
    // Capability holds but the anchor does NOT list this concrete → false.
    // Pins the `capability && contains(canonical)` conjunction against `||`
    // (which would return true on capability alone).
    let no_hit = graph_with_anchor("crate::ports::Handler::handle", target_capable_anchor(&[]));
    assert!(
        !no_hit.is_anchor_backed_concrete(concrete, "application", &adapters),
        "capability without containment → false"
    );
    // Both hold → true (positive control).
    let hit = graph_with_anchor(
        "crate::ports::Handler::handle",
        target_capable_anchor(&[concrete]),
    );
    assert!(
        hit.is_anchor_backed_concrete(concrete, "application", &adapters),
        "capability + containment → true"
    );
}

// ── is_inherited_default_match: guard chain ─────────────────────────────

/// Index where `crate::T::m` has a default body and `crate::Impl` is a
/// non-overriding impl of `crate::T` — the shape that makes a genuine
/// inherited-default match.
fn inherited_default_index() -> WorkspaceTypeIndex {
    let mut idx = WorkspaceTypeIndex::new();
    idx.trait_methods
        .insert("crate::T".to_string(), HashSet::from(["m".to_string()]));
    idx.trait_methods_with_default_body
        .insert(("crate::T".to_string(), "m".to_string()));
    // Empty override-set for `crate::Impl` → `impl_overrides_method` is
    // false (not the hand-built-index "assume override" default).
    idx.trait_impl_overrides.insert(
        "crate::T".to_string(),
        HashMap::from([("crate::Impl".to_string(), HashSet::new())]),
    );
    idx
}

#[test]
fn inherited_default_match_false_when_impl_not_listed() {
    // The impl_canon must appear in `impls`; an empty list short-circuits
    // to false. Pins `is_inherited_default_match` against `-> true`.
    let idx = inherited_default_index();
    assert!(
        !is_inherited_default_match(&idx, "crate::T", &[], "crate::Impl", "m"),
        "impl not in trait's impl list → false"
    );
}

#[test]
fn inherited_default_match_true_for_non_overriding_default_impl() {
    // Full match: impl listed, method on trait, default body, not
    // overridden → true (positive control for the guard chain).
    let idx = inherited_default_index();
    assert!(
        is_inherited_default_match(
            &idx,
            "crate::T",
            &["crate::Impl".to_string()],
            "crate::Impl",
            "m"
        ),
        "non-overriding default-body impl → true"
    );
}
