mod analyze_codebase;
mod architecture;
mod gates;
mod metrics;
mod pipeline;
mod projection;
mod projection_secondary;
mod run;
mod structural_metrics;
mod tq_metrics;
mod warnings;

/// One `test.rs` suppression at `line` covering `dim`. Shared by the per-pass
/// suppression-window tests (each pass's `mark_*_suppressions` is itself a
/// near-clone of the others).
pub(super) fn one_suppression(
    line: usize,
    dim: crate::findings::Dimension,
) -> std::collections::HashMap<String, Vec<crate::findings::Suppression>> {
    [(
        "test.rs".to_string(),
        vec![crate::findings::Suppression {
            line,
            dimensions: vec![dim],
            reason: None,
        }],
    )]
    .into()
}
