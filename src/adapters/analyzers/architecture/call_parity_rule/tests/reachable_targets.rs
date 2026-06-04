//! Unit test for `check_b_coverage::build_adapter_reachable_targets` — the
//! anchor-impl propagation step of the reachability BFS that drives Check B's
//! anchor-orphan suppression.

use crate::adapters::analyzers::architecture::call_parity_rule::anchor_index::AnchorInfo;
use crate::adapters::analyzers::architecture::call_parity_rule::check_b_coverage::build_adapter_reachable_targets;
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::CallGraph;
use std::collections::{HashMap, HashSet};

#[test]
fn reachable_targets_only_propagate_capability_anchor_impls() {
    // Anchor "A" (seeded via coverage) lists a phantom impl with a cached
    // layer but NO forward entry — NOT a target-capability node. The
    // `is_target_capability_node(..) && reachable.insert(..)` short-circuit
    // must keep it OUT of the reachable set. Pins that `&&` against `||`
    // (which would run the insert side-effect and add the phantom).
    let mut layer_of = HashMap::new();
    layer_of.insert("crate::app::A".to_string(), Some("application".to_string()));
    layer_of.insert(
        "crate::app::Phantom::m".to_string(),
        Some("application".to_string()),
    );
    let mut anchors = HashMap::new();
    anchors.insert(
        "crate::app::A".to_string(),
        AnchorInfo {
            impl_layers: HashSet::new(),
            impl_method_canonicals: HashSet::from(["crate::app::Phantom::m".to_string()]),
            decl_layer: Some("application".to_string()),
            has_default_body: false,
            trait_visible: true,
            location: None,
        },
    );
    let graph = CallGraph {
        forward: HashMap::new(), // no forward entry for Phantom::m → not a capability node
        reverse: HashMap::new(),
        layer_of,
        trait_method_anchors: anchors,
    };
    let mut coverage: HashMap<String, HashSet<String>> = HashMap::new();
    coverage.insert(
        "cli".to_string(),
        HashSet::from(["crate::app::A".to_string()]),
    );
    let adapters = vec!["cli".to_string(), "mcp".to_string()];
    let reachable = build_adapter_reachable_targets(&coverage, &graph, "application", &adapters);
    assert!(
        reachable.contains("crate::app::A"),
        "seeded anchor present: {reachable:?}"
    );
    assert!(
        !reachable.contains("crate::app::Phantom::m"),
        "non-capability phantom impl not propagated: {reachable:?}"
    );
}
