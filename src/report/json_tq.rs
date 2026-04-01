use super::json_types::JsonTqWarning;
use super::AnalysisResult;

/// Build JSON TQ warning entries from analysis results.
/// Operation: iteration + match on TQ warning kind, no own calls.
pub(super) fn build_tq_warnings(analysis: &AnalysisResult) -> Vec<JsonTqWarning> {
    analysis
        .tq
        .as_ref()
        .map(|tq| {
            tq.warnings
                .iter()
                .map(|w| {
                    let kind = match &w.kind {
                        crate::tq::TqWarningKind::NoAssertion => "no_assertion",
                        crate::tq::TqWarningKind::NoSut => "no_sut",
                        crate::tq::TqWarningKind::Untested => "untested",
                        crate::tq::TqWarningKind::Uncovered => "uncovered",
                        crate::tq::TqWarningKind::UntestedLogic { .. } => "untested_logic",
                    };
                    JsonTqWarning {
                        file: w.file.clone(),
                        line: w.line,
                        function_name: w.function_name.clone(),
                        kind: kind.to_string(),
                        suppressed: w.suppressed,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}
