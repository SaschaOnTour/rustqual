//! Per-dim Row types for the AI reporter. Each row holds a finding
//! clone plus the resolved function name; `format::*` helpers convert
//! the row to a JSON `Value`.

use crate::domain::findings::{
    ArchitectureFinding, ComplexityFinding, CouplingFinding, DryFinding, IospFinding, SrpFinding,
    TqFinding,
};

pub struct AiIospRow {
    pub(crate) finding: IospFinding,
    pub(crate) function_name: String,
}

pub struct AiComplexityRow {
    pub(crate) finding: ComplexityFinding,
    pub(crate) function_name: String,
}

pub struct AiDryRow {
    pub(crate) finding: DryFinding,
    pub(crate) function_name: String,
}

pub struct AiSrpRow {
    pub(crate) finding: SrpFinding,
    pub(crate) function_name: String,
}

pub struct AiCouplingRow {
    pub(crate) finding: CouplingFinding,
    pub(crate) function_name: String,
}

pub struct AiTqRow {
    pub(crate) finding: TqFinding,
    pub(crate) function_name: String,
}

pub struct AiArchRow {
    pub(crate) finding: ArchitectureFinding,
}
