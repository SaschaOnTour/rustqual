//! Per-dimension suppression window widths — the single source of
//! truth shared by the marking pipeline and the orphan detector.
//!
//! Each constant describes how far above a finding a
//! `// qual:allow(...)` marker can sit and still count as suppressing
//! it. The marking code (`app::metrics::mark_*`,
//! `app::structural_metrics::mark_*`, `app::tq_metrics::mark_*`,
//! `adapters::suppression::qual_allow::is_within_window`) and the
//! orphan detector (`app::orphan_suppressions`) both read from this
//! module. Without that sharing the two paths would drift — the
//! marker would suppress a finding that the orphan detector then
//! reports as orphan (or vice versa).
//!
//! Semantics: a suppression on line `sup_line` matches a finding on
//! line `finding_line` iff
//! `sup_line <= finding_line && finding_line - sup_line <= WINDOW`.

/// Default window for IOSP violations, Complexity warnings, and the
/// majority of DRY findings (duplicates, fragments, boilerplate,
/// repeated matches, plus the `qual:api` / `qual:test_helper`
/// annotations).
pub(crate) const DEFAULT: usize = crate::findings::ANNOTATION_WINDOW;

/// SRP struct + parameter warnings. Wider than the default so a
/// `qual:allow(srp)` above a `#[derive(...)]` attribute group still
/// reaches the struct or function below it.
pub(crate) const SRP_STRUCT_PARAM: usize = 5;

/// Test-quality warnings. Matches the hard-coded `5` in
/// `app::tq_metrics::mark_tq_suppressions`.
pub(crate) const TQ: usize = 5;

/// Structural binary-check warnings (OI, SIT, DEH, IET, BTC, SLM, NMS).
/// Matches the hard-coded `5` in
/// `app::structural_metrics::mark_structural_suppressions`.
pub(crate) const STRUCTURAL: usize = 5;

/// Wildcard-import warnings. Tightest window — a marker on the same
/// line as the `use mod::*;` or exactly one line above it.
/// `app::metrics::mark_wildcard_suppressions` enforces the same.
pub(crate) const WILDCARD: usize = 1;
