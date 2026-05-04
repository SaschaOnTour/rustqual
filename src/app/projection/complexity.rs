//! Complexity-dimension projection: per-function warning flags →
//! typed `Vec<ComplexityFinding>`. Multiple flags per function produce
//! multiple findings; magic numbers produce one finding per occurrence.

use crate::adapters::analyzers::iosp::{ComplexityMetrics, FunctionAnalysis};
use crate::config::Config;
use crate::domain::findings::{ComplexityFinding, ComplexityFindingKind, ComplexityHotspotDetail};
use crate::domain::{Dimension, Finding, Severity};

const DIM: Dimension = Dimension::Complexity;
const SEV: Severity = Severity::Medium;

/// Project Complexity warnings (cognitive, cyclomatic, nesting, length,
/// magic-number, unsafe, error-handling) into typed ComplexityFinding
/// entries. Functions with `complexity_suppressed = true` are skipped
/// (they wouldn't carry warning flags either way, but the guard makes
/// the contract explicit).
pub(crate) fn project_complexity(
    results: &[FunctionAnalysis],
    config: &Config,
) -> Vec<ComplexityFinding> {
    let mut out = Vec::new();
    for f in results {
        if f.complexity_suppressed {
            continue;
        }
        push_threshold_findings(&mut out, f, config);
        push_magic_number_findings(&mut out, f);
    }
    out
}

fn push_threshold_findings(
    out: &mut Vec<ComplexityFinding>,
    f: &FunctionAnalysis,
    config: &Config,
) {
    let metrics = f.complexity.as_ref();
    push_metric_threshold(
        out,
        f,
        f.cognitive_warning,
        ComplexityFindingKind::Cognitive,
        metrics.map_or(0, |m| m.cognitive_complexity),
        config.complexity.max_cognitive,
        None,
    );
    push_metric_threshold(
        out,
        f,
        f.cyclomatic_warning,
        ComplexityFindingKind::Cyclomatic,
        metrics.map_or(0, |m| m.cyclomatic_complexity),
        config.complexity.max_cyclomatic,
        None,
    );
    push_metric_threshold(
        out,
        f,
        f.nesting_depth_warning,
        ComplexityFindingKind::NestingDepth,
        metrics.map_or(0, |m| m.max_nesting),
        config.complexity.max_nesting_depth,
        nesting_hotspot(metrics),
    );
    push_metric_threshold(
        out,
        f,
        f.function_length_warning,
        ComplexityFindingKind::FunctionLength,
        metrics.map_or(0, |m| m.function_lines),
        config.complexity.max_function_lines,
        None,
    );
    push_metric_threshold(
        out,
        f,
        f.unsafe_warning,
        ComplexityFindingKind::Unsafe,
        metrics.map_or(0, |m| m.unsafe_blocks),
        0,
        None,
    );
    push_metric_threshold(
        out,
        f,
        f.error_handling_warning,
        ComplexityFindingKind::ErrorHandling,
        metrics.map_or(0, error_handling_count),
        0,
        None,
    );
}

// qual:allow(srp) reason: "single-purpose accumulator: gate-test + push the typed finding; parameters are inherent to the per-metric uniform pattern"
fn push_metric_threshold(
    out: &mut Vec<ComplexityFinding>,
    f: &FunctionAnalysis,
    flag: bool,
    kind: ComplexityFindingKind,
    metric_value: usize,
    threshold_value: usize,
    hotspot: Option<ComplexityHotspotDetail>,
) {
    if flag {
        out.push(threshold(f, kind, metric_value, threshold_value, hotspot));
    }
}

fn nesting_hotspot(metrics: Option<&ComplexityMetrics>) -> Option<ComplexityHotspotDetail> {
    metrics
        .and_then(|m| m.hotspots.first())
        .map(|h| ComplexityHotspotDetail {
            line: h.line,
            nesting_depth: h.nesting_depth,
            construct: h.construct.clone(),
        })
}

fn error_handling_count(m: &ComplexityMetrics) -> usize {
    m.unwrap_count + m.expect_count + m.panic_count + m.todo_count
}

fn push_magic_number_findings(out: &mut Vec<ComplexityFinding>, f: &FunctionAnalysis) {
    if f.is_test {
        return;
    }
    let Some(metrics) = f.complexity.as_ref() else {
        return;
    };
    let meta = ComplexityFindingKind::MagicNumber.meta();
    metrics.magic_numbers.iter().for_each(|mn| {
        out.push(ComplexityFinding {
            common: Finding {
                file: f.file.clone(),
                line: mn.line,
                column: 0,
                dimension: DIM,
                rule_id: meta.rule_id.into(),
                message: format!("{} {} in {}", meta.description, mn.value, f.qualified_name),
                severity: SEV,
                suppressed: f.suppressed,
            },
            kind: ComplexityFindingKind::MagicNumber,
            metric_value: 1,
            threshold: 0,
            hotspot: None,
        });
    });
}

fn threshold(
    f: &FunctionAnalysis,
    kind: ComplexityFindingKind,
    metric_value: usize,
    threshold: usize,
    hotspot: Option<ComplexityHotspotDetail>,
) -> ComplexityFinding {
    let meta = kind.meta();
    ComplexityFinding {
        common: Finding {
            file: f.file.clone(),
            line: f.line,
            column: 0,
            dimension: DIM,
            rule_id: meta.rule_id.into(),
            message: format!(
                "{} {metric_value} in {}",
                meta.description, f.qualified_name
            ),
            severity: SEV,
            suppressed: f.suppressed,
        },
        kind,
        metric_value,
        threshold,
        hotspot,
    }
}
