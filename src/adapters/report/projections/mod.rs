//! Shared projection helpers used by reporters.
//!
//! Each reporter has its own View struct (per-reporter layout) but the
//! underlying projection from typed Findings into structured rows is
//! reporter-agnostic — that's what lives here. Atomic row types
//! (`StructuralRow`, `SrpStructRow`, `SdpViolationRow`, etc.) are
//! shared across reporters; the View aggregators that bundle them
//! into a per-section payload remain per-reporter.

pub(crate) mod coupling;
pub(crate) mod dry;
pub(crate) mod srp;
pub(crate) mod tq;
