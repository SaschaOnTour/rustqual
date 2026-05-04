//! Per-dimension typed Finding structures.
//!
//! Each of rustqual's seven dimensions emits its own typed Finding shape
//! (`IospFinding`, `ComplexityFinding`, `DryFinding`, `SrpFinding`,
//! `CouplingFinding`, `TqFinding`, `ArchitectureFinding`). All embed
//! `domain::Finding` as the `common` field for shared metadata
//! (file/line/column/dimension/rule_id/message/severity/suppressed) and
//! add dimension-specific detail on top.
//!
//! The `AnalysisFindings` aggregate combines all seven into a single
//! payload that every Reporter consumes via the `ports::reporter::Reporter`
//! trait. Adding a new dimension means: new finding struct here, new
//! field on `AnalysisFindings`, new method on the Reporter trait —
//! every existing Reporter then fails to compile until migrated, giving
//! compile-time Reporter-Parity.

pub mod aggregate;
pub mod architecture;
pub mod complexity;
pub mod coupling;
pub mod dry;
pub mod iosp;
pub mod orphan;
pub mod srp;
pub mod tq;

pub use aggregate::AnalysisFindings;
pub use architecture::ArchitectureFinding;
pub use complexity::{ComplexityFinding, ComplexityFindingKind, ComplexityHotspotDetail};
pub use coupling::{CouplingFinding, CouplingFindingDetails, CouplingFindingKind};
pub use dry::{
    DryFinding, DryFindingDetails, DryFindingKind, DuplicateParticipant, FragmentParticipant,
    RepeatedMatchParticipant,
};
pub use iosp::{CallLocation, IospFinding, LogicLocation};
pub use orphan::OrphanSuppression;
pub use srp::{SrpFinding, SrpFindingDetails, SrpFindingKind};
pub use tq::{TqFinding, TqFindingKind};
