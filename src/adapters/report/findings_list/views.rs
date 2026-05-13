//! Per-dim row types for the findings-list reporter. Each row holds
//! the structured data needed to project into a `FindingEntry` —
//! `format::*` helpers consume them in `publish`.

use crate::domain::findings::{
    ComplexityFinding, CouplingFinding, DryFinding, SrpFinding, TqFindingKind,
};

pub struct ListIospRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) function_name: String,
}

pub struct ListComplexityRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) function_name: String,
    pub(crate) finding: ComplexityFinding,
}

pub struct ListDryRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) function_name: String,
    pub(crate) finding: DryFinding,
}

pub struct ListSrpRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) function_name: String,
    pub(crate) finding: SrpFinding,
}

pub struct ListCouplingRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) function_name: String,
    pub(crate) finding: CouplingFinding,
}

pub struct ListTqRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) function_name: String,
    pub(crate) kind: TqFindingKind,
}

pub struct ListArchRow {
    pub(crate) file: String,
    pub(crate) line: usize,
    pub(crate) message: String,
}
