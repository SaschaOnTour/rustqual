//! Per-dim row → JSON entry conversion.

use serde_json::{json, Value};

use super::details::{coupling_category_detail, dry_category_detail, srp_category_detail};
use super::rows::{
    AiArchRow, AiComplexityRow, AiCouplingRow, AiDryRow, AiIospRow, AiSrpRow, AiTqRow,
};
use crate::config::Config;
use crate::domain::findings::ComplexityFindingKind;

pub(crate) fn format_iosp_entry(r: AiIospRow) -> Value {
    let logic_lines: Vec<String> = r
        .finding
        .logic_locations
        .iter()
        .map(|l| l.line.to_string())
        .collect();
    let call_lines: Vec<String> = r
        .finding
        .call_locations
        .iter()
        .map(|c| c.line.to_string())
        .collect();
    let detail = format!(
        "logic + calls (logic lines {}, call lines {})",
        logic_lines.join(","),
        call_lines.join(","),
    );
    build_value_entry(
        &r.finding.common.file,
        r.finding.common.line,
        &r.function_name,
        "violation",
        detail,
    )
}

pub(crate) fn format_complexity_entry(r: AiComplexityRow) -> Value {
    let category = r.finding.kind.meta().ai_category;
    let detail = match r.finding.kind {
        ComplexityFindingKind::Cognitive
        | ComplexityFindingKind::Cyclomatic
        | ComplexityFindingKind::NestingDepth
        | ComplexityFindingKind::FunctionLength => {
            format!("{} (max {})", r.finding.metric_value, r.finding.threshold)
        }
        ComplexityFindingKind::MagicNumber
        | ComplexityFindingKind::Unsafe
        | ComplexityFindingKind::ErrorHandling => r.finding.common.message.clone(),
    };
    build_value_entry(
        &r.finding.common.file,
        r.finding.common.line,
        &r.function_name,
        category,
        detail,
    )
}

pub(crate) fn format_dry_entry(r: AiDryRow) -> Value {
    let (category, detail) = dry_category_detail(&r.finding);
    build_value_entry(
        &r.finding.common.file,
        r.finding.common.line,
        &r.function_name,
        category,
        detail,
    )
}

pub(crate) fn format_srp_entry(r: AiSrpRow, config: &Config) -> Value {
    let (category, detail) = srp_category_detail(&r.finding, config);
    build_value_entry(
        &r.finding.common.file,
        r.finding.common.line,
        &r.function_name,
        category,
        detail,
    )
}

pub(crate) fn format_coupling_entry(r: AiCouplingRow) -> Value {
    let (category, detail) = coupling_category_detail(&r.finding);
    build_value_entry(
        &r.finding.common.file,
        r.finding.common.line,
        &r.function_name,
        category,
        detail,
    )
}

pub(crate) fn format_tq_entry(r: AiTqRow) -> Value {
    let category = r.finding.kind.meta().ai_category;
    build_value_entry(
        &r.finding.common.file,
        r.finding.common.line,
        &r.function_name,
        category,
        r.finding.common.message.clone(),
    )
}

pub(crate) fn format_arch_entry(r: AiArchRow) -> Value {
    let detail = format!("{}: {}", r.finding.common.rule_id, r.finding.common.message);
    build_value_entry(
        &r.finding.common.file,
        r.finding.common.line,
        "",
        "architecture",
        detail,
    )
}

fn build_value_entry(
    file: &str,
    line: usize,
    function_name: &str,
    category: &str,
    detail: String,
) -> Value {
    json!({
        "file": file,
        "category": category,
        "line": line,
        "fn": function_name,
        "detail": detail,
    })
}
