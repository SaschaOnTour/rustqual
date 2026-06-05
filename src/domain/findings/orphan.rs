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

/// Why a `// qual:allow(...)` marker is being reported. A *stale* marker
/// matched no finding (or is malformed); a *too-loose* marker is a metric
/// pin parked further above the value it covers than `pin_headroom` allows.
/// The two render with different lead words so the author knows whether to
/// delete the marker or merely tighten its pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OrphanKind {
    /// Marker matched no finding of its (dimension, target) — stale or
    /// misplaced. Also used for malformed markers surfaced via the
    /// invalid-marker side-channel.
    #[default]
    Stale,
    /// A metric pin that *does* cover a finding, but sits more than
    /// `pin_headroom` above that finding's value — tighten or remove.
    PinTooLoose,
}

/// A `// qual:allow(...)` marker that should be reported: a stale/misplaced
/// suppression, or a metric pin that is looser than it needs to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanSuppression {
    pub file: String,
    /// 1-based line of the marker (already shifted to the last line
    /// of the contiguous `//`-comment block containing the marker).
    pub line: usize,
    /// Which dimensions the marker tried to suppress. Empty only for a
    /// malformed marker surfaced via the invalid-marker side-channel (e.g.
    /// unclosed parens); a real parsed `Suppression` always carries at least
    /// one dimension, so this is non-empty for stale/too-loose orphans.
    pub dimensions: Vec<Dimension>,
    /// Optional human-readable rationale attached to the marker (for a
    /// too-loose pin, this carries the tighten-to-~value advice instead).
    pub reason: Option<String>,
    /// Why the marker is being reported (stale vs too-loose pin).
    pub kind: OrphanKind,
}

impl OrphanSuppression {
    /// Lead word for reports: `stale` for an unmatched/malformed marker,
    /// `too-loose` for a metric pin above its headroom. Centralised so
    /// every reporter renders the same word for the same kind.
    pub fn status_word(&self) -> &'static str {
        match self.kind {
            OrphanKind::Stale => "stale",
            OrphanKind::PinTooLoose => "too-loose",
        }
    }
}
