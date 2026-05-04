//! Format helpers — convert per-dim view rows into public `FindingEntry`
//! shapes. The detail-string and category-label formatting that
//! happens here is the pure-data Views' counterpart to projection in
//! `build_*`.

use super::categories::{
    complexity_category, complexity_detail, coupling_category_detail, dry_category_detail,
    srp_category_detail, tq_category,
};
use super::views::{
    ListArchRow, ListComplexityRow, ListCouplingRow, ListDryRow, ListIospRow, ListSrpRow, ListTqRow,
};
use super::FindingEntry;

pub(super) fn format_iosp(r: ListIospRow) -> FindingEntry {
    FindingEntry::new(
        &r.file,
        r.line,
        "VIOLATION",
        "logic + calls".into(),
        r.function_name,
    )
}

pub(super) fn format_complexity(r: ListComplexityRow) -> FindingEntry {
    FindingEntry::new(
        &r.file,
        r.line,
        complexity_category(r.finding.kind),
        complexity_detail(&r.finding),
        r.function_name,
    )
}

pub(super) fn format_dry(r: ListDryRow) -> FindingEntry {
    let (category, detail) = dry_category_detail(&r.finding);
    FindingEntry::new(&r.file, r.line, category, detail, r.function_name)
}

pub(super) fn format_srp(r: ListSrpRow) -> FindingEntry {
    let (category, detail) = srp_category_detail(&r.finding);
    FindingEntry::new(&r.file, r.line, category, detail, r.function_name)
}

pub(super) fn format_coupling(r: ListCouplingRow) -> FindingEntry {
    let (category, detail) = coupling_category_detail(&r.finding);
    FindingEntry::new(&r.file, r.line, category, detail, r.function_name)
}

pub(super) fn format_tq(r: ListTqRow) -> FindingEntry {
    FindingEntry::new(
        &r.file,
        r.line,
        tq_category(&r.kind),
        r.function_name.clone(),
        r.function_name,
    )
}

pub(super) fn format_architecture(r: ListArchRow) -> FindingEntry {
    FindingEntry::new(&r.file, r.line, "ARCHITECTURE", r.message, String::new())
}
