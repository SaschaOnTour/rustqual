use super::json_types::JsonStructuralWarning;
use super::AnalysisResult;
use crate::findings::Dimension;

/// Build JSON structural warning entries from analysis results.
/// Operation: iteration + method calls via closure, no own calls.
pub(super) fn build_structural_warnings(analysis: &AnalysisResult) -> Vec<JsonStructuralWarning> {
    analysis
        .structural
        .as_ref()
        .map(|s| {
            s.warnings
                .iter()
                .filter(|w| !w.suppressed)
                .map(|w| {
                    let (code, detail) = (w.kind.code(), w.kind.detail());
                    let dimension = match w.dimension {
                        Dimension::Srp => "srp",
                        Dimension::Coupling => "coupling",
                        _ => "srp",
                    };
                    JsonStructuralWarning {
                        file: w.file.clone(),
                        line: w.line,
                        name: w.name.clone(),
                        code: code.to_string(),
                        dimension: dimension.to_string(),
                        detail,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}
