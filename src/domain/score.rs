//! Score-related value types and constants.
//!
//! Quality scores throughout rustqual live on the unit interval `0.0..=1.0`
//! but are rendered to users as percentages. `PERCENTAGE_MULTIPLIER` is the
//! single conversion constant every layer uses when moving between the two
//! representations.

/// Multiplier for converting score ratio (0.0–1.0) to percentage (0–100).
pub const PERCENTAGE_MULTIPLIER: f64 = 100.0;
