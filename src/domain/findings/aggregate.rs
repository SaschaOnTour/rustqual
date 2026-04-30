//! Aggregate of all per-dimension findings produced by an analysis run.
//!
//! `AnalysisFindings` is the unified payload that every Reporter consumes.
//! Each dimension owns one Vec; reporters iterate the dimensions they
//! render, and the Reporter trait (in `ports::reporter`) requires a
//! method per dimension — so adding a new dimension is a compile-time
//! error in every reporter that hasn't been migrated.
//!
//! State-of-codebase data (per-module instability, classification ratios,
//! cluster topologies) lives in the dimension-specific report structs in
//! `adapters::analyzers::*` — not here. This struct is for findings only.

use super::{
    architecture::ArchitectureFinding, complexity::ComplexityFinding, coupling::CouplingFinding,
    dry::DryFinding, iosp::IospFinding, srp::SrpFinding, tq::TqFinding,
};

/// All findings of an analysis run, grouped by dimension.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnalysisFindings {
    pub iosp: Vec<IospFinding>,
    pub complexity: Vec<ComplexityFinding>,
    pub dry: Vec<DryFinding>,
    pub srp: Vec<SrpFinding>,
    pub coupling: Vec<CouplingFinding>,
    pub test_quality: Vec<TqFinding>,
    pub architecture: Vec<ArchitectureFinding>,
}
