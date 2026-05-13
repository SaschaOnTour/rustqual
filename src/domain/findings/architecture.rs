//! Typed Finding for the Architecture dimension.
//!
//! Architecture findings carry no dimension-specific data beyond what
//! `Finding` already provides — the rule_id (`architecture/layer/...`,
//! `architecture/forbidden/...`, `architecture/call_parity/...`,
//! `architecture/trait_contract/...`) carries the semantic, and the
//! message string describes the specific instance.
//!
//! `ArchitectureFinding` is therefore a thin wrapper over `Finding` for
//! API consistency with the other dimensions.

use crate::domain::Finding;

/// Architecture finding — layer violation, forbidden edge, pattern
/// violation, trait-contract breach, or call-parity issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitectureFinding {
    /// Common metadata. `common.dimension == Dimension::Architecture`.
    pub common: Finding,
}
