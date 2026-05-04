//! JSON `functions` section builder. Reads from typed `FunctionRecord`
//! state plus `IospFinding` violation locations.

use std::collections::BTreeMap;

use super::super::json_types::{JsonComplexity, JsonFunction, JsonHotspot, JsonMagicNumber};
use crate::domain::analysis_data::{FunctionClassification, FunctionRecord};
use crate::domain::findings::IospFinding;

pub(super) fn build_functions(
    functions: &[FunctionRecord],
    iosp_findings: &[IospFinding],
) -> Vec<JsonFunction> {
    functions
        .iter()
        .map(|f| {
            let (logic, calls) = violation_locations(f, iosp_findings);
            JsonFunction {
                name: f.name.clone(),
                file: f.file.clone(),
                line: f.line,
                parent_type: f.parent_type.clone(),
                classification: classification_str(f.classification).to_string(),
                severity: f.severity.clone(),
                suppressed: if f.suppressed { Some(true) } else { None },
                logic,
                calls,
                complexity: f.complexity.as_ref().map(|c| JsonComplexity {
                    logic_count: 0,
                    call_count: 0,
                    max_nesting: c.max_nesting,
                    cognitive_complexity: c.cognitive_complexity,
                    cyclomatic_complexity: c.cyclomatic_complexity,
                    function_lines: c.function_lines,
                    unsafe_blocks: c.unsafe_blocks,
                    unwrap_count: c.unwrap_count,
                    expect_count: c.expect_count,
                    panic_count: c.panic_count,
                    todo_count: c.todo_count,
                    hotspots: c
                        .hotspots
                        .iter()
                        .map(|h| JsonHotspot {
                            line: h.line,
                            nesting_depth: h.nesting_depth,
                            construct: h.construct.clone(),
                        })
                        .collect(),
                    magic_numbers: c
                        .magic_numbers
                        .iter()
                        .map(|m| JsonMagicNumber {
                            line: m.line,
                            value: m.value.clone(),
                        })
                        .collect(),
                }),
                parameter_count: f.parameter_count,
                is_trait_impl: f.is_trait_impl,
                effort_score: f.effort_score,
            }
        })
        .collect()
}

fn classification_str(c: FunctionClassification) -> &'static str {
    match c {
        FunctionClassification::Integration => "integration",
        FunctionClassification::Operation => "operation",
        FunctionClassification::Trivial => "trivial",
        FunctionClassification::Violation => "violation",
    }
}

#[allow(clippy::type_complexity)]
fn violation_locations(
    f: &FunctionRecord,
    iosp_findings: &[IospFinding],
) -> (Vec<BTreeMap<String, String>>, Vec<BTreeMap<String, String>>) {
    let matching = iosp_findings
        .iter()
        .find(|finding| finding.common.file == f.file && finding.common.line == f.line);
    let Some(m) = matching else {
        return (vec![], vec![]);
    };
    let logic = m
        .logic_locations
        .iter()
        .map(|l| {
            let mut entry = BTreeMap::new();
            entry.insert("kind".into(), l.kind.clone());
            entry.insert("line".into(), l.line.to_string());
            entry
        })
        .collect();
    let calls = m
        .call_locations
        .iter()
        .map(|c| {
            let mut entry = BTreeMap::new();
            entry.insert("name".into(), c.name.clone());
            entry.insert("line".into(), c.line.to_string());
            entry
        })
        .collect();
    (logic, calls)
}
