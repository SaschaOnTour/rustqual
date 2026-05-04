//! Trait-method anchor index for the call-parity call graph.
//!
//! Builds and queries `AnchorInfo` per `<Trait>::<method>` synthetic
//! node emitted by `dyn Trait.method()` dispatch. The unified target-
//! capability rule (`is_anchor_target_capability`) is the single
//! source of truth shared by the boundary walker and the Check B/D
//! capability enumeration — without that sharing, the two sides drift
//! (parallel-path inconsistency).

use super::type_infer::{MethodLocation, WorkspaceTypeIndex};
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use std::collections::HashSet;

/// One trait-method anchor's resolved metadata. Built at graph-build
/// time; consumed by the unified `is_anchor_target_capability` rule
/// (walker + `target_anchor_capabilities`) and by anchor-finding
/// rendering.
#[derive(Debug, Clone)]
pub(crate) struct AnchorInfo {
    /// Set of layer names where overriding impls of this trait method
    /// live. Empty for default-only traits.
    pub(crate) impl_layers: HashSet<String>,
    /// Canonical names of the overriding impl methods (`<Impl>::<method>`).
    /// Used by Check B/D to skip those concrete pub-fn entries — the
    /// anchor takes capability responsibility.
    pub(crate) impl_method_canonicals: HashSet<String>,
    /// Layer where the trait declaration itself lives. Denormalised
    /// (could be re-resolved via `LayerDefinitions::layer_of_crate_path`
    /// from the anchor key) so the unified target-capability rule
    /// stays a single predicate-only call on the hot path.
    pub(crate) decl_layer: Option<String>,
    /// True iff the trait method declares a default body. Methods
    /// without a default body and without an overriding impl are
    /// uncallable signatures and do NOT count as a target capability
    /// just because the trait happens to live in the target layer.
    pub(crate) has_default_body: bool,
    /// Source location of the trait method declaration. `None` when
    /// the trait was registered without a captured span (synthetic
    /// test fixtures).
    pub(crate) location: Option<MethodLocation>,
}

/// **Unified target-capability rule for trait-method anchors.** Both
/// the boundary walker (`TouchpointWalk::is_target_boundary`) and the
/// Check B/D enumeration (`CallGraph::target_anchor_capabilities`)
/// MUST consult this same predicate, otherwise the two sides drift
/// (parallel-path inconsistency — see memory pattern A18).
///
/// Returns `true` iff: (1) the trait's declaring layer is NOT a peer
/// adapter (a configured adapter that isn't the target), AND (2) the
/// trait's declaring layer IS the target layer AND the method has a
/// callable body (default OR overriding impl), OR at least one
/// overriding impl lives in the target layer.
///
/// Rule 1 prevents `cli` from inheriting `mcp::Handler`-backed coverage
/// when the trait is declared in the `mcp` peer adapter. Rule 2 covers
/// the Hexagonal layout (trait in `ports`, impls in `application`),
/// the default-only-target case (trait + default body in target, no
/// overriding impls anywhere), and rejects pure-signature trait methods
/// in target (no default, no impl) which are uncallable and not a
/// capability. Operation: predicate logic.
pub(crate) fn is_anchor_target_capability(
    info: &AnchorInfo,
    target_layer: &str,
    adapter_layers: &[String],
) -> bool {
    if let Some(decl) = info.decl_layer.as_deref() {
        if decl != target_layer && adapter_layers.iter().any(|a| a == decl) {
            return false;
        }
        if decl == target_layer
            && (info.has_default_body || info.impl_layers.contains(target_layer))
        {
            return true;
        }
    }
    info.impl_layers.contains(target_layer)
}

/// Construct one anchor's `AnchorInfo` from the workspace type index
/// plus layer definitions: collect overriding impl canonicals, derive
/// their layers, look up the trait method's source location, and the
/// default-body flag. Operation: per-method assembly.
pub(crate) fn build_anchor_info(
    type_index: &WorkspaceTypeIndex,
    layers: &LayerDefinitions,
    trait_canonical: &str,
    method: &str,
    decl_layer: &Option<String>,
) -> AnchorInfo {
    let overriding = type_index.overriding_impls_for(trait_canonical, method);
    let impl_layers: HashSet<String> = overriding
        .iter()
        .filter_map(|impl_canon| layers.layer_of_crate_path(impl_canon).map(String::from))
        .collect();
    let impl_method_canonicals: HashSet<String> = overriding
        .iter()
        .map(|impl_canon| format!("{impl_canon}::{method}"))
        .collect();
    let location = type_index
        .trait_method_location(trait_canonical, method)
        .cloned();
    let has_default_body = type_index.trait_method_has_default_body(trait_canonical, method);
    AnchorInfo {
        impl_layers,
        impl_method_canonicals,
        decl_layer: decl_layer.clone(),
        has_default_body,
        location,
    }
}
