//! Workspace-level tests for `pub use` re-export resolution. Mirrors the
//! external repros: non-pub helper canonicalisation (v1.2.2), `pub use` of a
//! free fn (v1.2.4), `pub use Trait;` + generic dispatch routing to the trait
//! DECL canonical, impl-Self-type resolution through re-export, value-reexport
//! trait-bound guard, and `pub use Struct;` associated-fn routing. Split into
//! focused sub-files (each ≤ the SRP file-length cap); shared imports reach the
//! sub-modules via `use super::*`.

pub(super) use super::support::{
    build_graph_only, build_workspace, callees_of, cli_mcp_config, empty_cfg_test,
    graph_contains_edge, run_check_b, three_layer,
};
pub(super) use std::collections::HashSet;

mod basic_and_trait_dispatch;
mod impl_and_value_reexport;
