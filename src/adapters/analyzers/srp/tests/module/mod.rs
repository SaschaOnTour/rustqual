//! Module-level SRP tests: production-line counting, file-length scoring,
//! `analyze_module_srp`, free-function collection, and independent-cluster
//! detection. Split into focused sub-files (each ≤ the SRP file-length cap);
//! shared imports live here and reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::srp::module::*;
pub(super) use crate::config::sections::SrpConfig;
pub(super) use std::collections::HashMap;
pub(super) use syn::visit::Visit;

mod clusters;
mod production_lines;
mod scoring_and_analysis;
