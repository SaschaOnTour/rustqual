//! Orphan-suppression Finding type.
//!
//! Cross-cutting Finding produced when a `// qual:allow(...)` marker
//! fails to match any real finding inside its annotation window
//! (a stale or misplaced suppression). Lives in `domain::findings`
//! alongside the per-dimension Finding types so the Reporter port can
//! treat orphan rendering as a compile-time-required projection
//! (`ReporterImpl::OrphanView` + `build_orphans`), preventing future
//! reporters from silently omitting orphan output.

use crate::domain::{Dimension, SuppressionTarget};

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

impl OrphanKind {
    /// Lead word for human reports: `stale` for an unmatched/malformed marker,
    /// `too-loose` for a metric pin above its headroom. Centralised so every
    /// reporter renders the same word for the same kind.
    pub fn status_word(self) -> &'static str {
        match self {
            OrphanKind::Stale => "stale",
            OrphanKind::PinTooLoose => "too-loose",
        }
    }

    /// Stable snake_case token for structured output (JSON/AI), so machine
    /// consumers can branch on the remedy — `stale` → delete the marker,
    /// `too_loose` → tighten the pin — without parsing the human message.
    pub fn json_kind(self) -> &'static str {
        match self {
            OrphanKind::Stale => "stale",
            OrphanKind::PinTooLoose => "too_loose",
        }
    }
}

/// A `// qual:allow(...)` marker that should be reported: a stale/misplaced
/// suppression, or a metric pin that is looser than it needs to be.
///
/// (No `Eq`: `target` carries a metric pin (`f64`), which is `PartialEq` only.)
#[derive(Debug, Clone, PartialEq)]
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
    /// The marker's target, when it was a *targeted* `allow(dim, target[=N])`.
    /// Carried so reporters render which finding-kind is stale/too-loose
    /// (`qual:allow(srp, file_length=400)`), not just the dimension — vital
    /// when a file has several markers of the same dimension.
    pub target: Option<SuppressionTarget>,
    /// Optional human-readable rationale attached to the marker (for a
    /// too-loose pin, this carries the tighten-to-~value advice instead).
    pub reason: Option<String>,
    /// Why the marker is being reported (stale vs too-loose pin).
    pub kind: OrphanKind,
}

impl OrphanSuppression {
    /// The marker's target as a `qual:allow` argument — `"file_length=400"`
    /// for a metric pin, `"god_struct"` for a boolean target, `None` for a
    /// blanket/invalid marker. The structured form for JSON-style output.
    pub fn target_spec(&self) -> Option<String> {
        self.target.as_ref().map(|t| match t.pin() {
            Some(pin) => format!("{}={}", t.name(), fmt_pin(pin)),
            None => t.name().to_string(),
        })
    }

    /// The target rendered as a trailing argument for the `qual:allow(<dims>…)`
    /// spec — `""` for a blanket/invalid marker, `", file_length=400"` for a
    /// metric pin. Reporters append this to their dimension scope so a targeted
    /// orphan names its finding-kind.
    pub fn target_suffix(&self) -> String {
        self.target_spec()
            .map(|s| format!(", {s}"))
            .unwrap_or_default()
    }
}

/// Render a pin without a trailing `.0` for whole numbers (mirrors the
/// orphan-detector's `fmt_num`).
fn fmt_pin(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}
