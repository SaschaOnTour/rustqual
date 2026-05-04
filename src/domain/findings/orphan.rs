//! Orphan-suppression Finding type.
//!
//! Cross-cutting Finding produced when a `// qual:allow(...)` marker
//! fails to match any real finding inside its annotation window
//! (a stale or misplaced suppression). Lives in `domain::findings`
//! alongside the per-dimension Finding types so the Reporter port can
//! treat orphan rendering as a compile-time-required projection
//! (`ReporterImpl::OrphanView` + `build_orphans`), preventing future
//! reporters from silently omitting orphan output.

use crate::domain::Dimension;

/// A `// qual:allow(...)` marker that failed to match any finding in
/// its annotation window. Represents a stale or misplaced suppression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanSuppression {
    pub file: String,
    /// 1-based line of the marker (already shifted to the last line
    /// of the contiguous `//`-comment block containing the marker).
    pub line: usize,
    /// Which dimensions the marker tried to suppress. Empty = wildcard
    /// (bare `// qual:allow`).
    pub dimensions: Vec<Dimension>,
    /// Optional human-readable rationale attached to the marker.
    pub reason: Option<String>,
}
