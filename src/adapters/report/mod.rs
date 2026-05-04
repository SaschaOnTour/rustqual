// qual:allow(coupling) reason: "report naturally depends on all analysis modules"
mod ai;
mod baseline;
mod dot;
mod dry_dedup;
pub(crate) mod findings_list;
mod github;
mod html;
mod json;
mod json_types;
mod projections;
mod sarif;
mod suggestions;
pub mod text;

pub use ai::{print_ai, print_ai_json};
pub use baseline::{create_baseline, print_comparison};
pub use dot::print_dot;
pub use github::print_github;
// print_dry_section re-exported below
pub use html::print_html;
pub use json::print_json;
pub use sarif::print_sarif;
pub use suggestions::print_suggestions;
pub use text::{print_files_only, print_summary_only, print_text};

use crate::adapters::analyzers::iosp::{Classification, FunctionAnalysis};

/// All outputs from a full analysis run.
pub struct AnalysisResult {
    /// Per-function IOSP analysis records — classification, complexity
    /// metrics, severity, suppression flags. Source-of-truth for IOSP +
    /// Complexity reporting; the typed projections under `findings.iosp`
    /// and `findings.complexity` are derived from this.
    pub results: Vec<FunctionAnalysis>,
    /// Summary statistics: dimension scores, finding counts, total
    /// suppressions. Populated by the analysis pipeline.
    pub summary: Summary,
    /// Per-dimension typed findings — the unified payload that the
    /// `Reporter` trait (in `ports::reporter`) consumes. Populated by
    /// projection adapters during analysis. Includes the cross-cutting
    /// `orphan_suppressions` field carrying `// qual:allow(...)` markers
    /// that matched no finding in their annotation window.
    pub findings: crate::domain::AnalysisFindings,
    /// Typed state-of-codebase data — counterpart to `findings`, the
    /// payload `AnalysisReporter` consumes. Carries per-function
    /// classifications + raw complexity metrics, per-module coupling
    /// records.
    pub data: crate::domain::AnalysisData,
}

/// Summary statistics for a full analysis run.
#[derive(Debug, Default)]
pub struct Summary {
    pub total: usize,
    pub integrations: usize,
    pub operations: usize,
    pub violations: usize,
    pub trivial: usize,
    pub suppressed: usize,
    /// IOSP compliance score (0.0 = all violations, 1.0 = fully compliant).
    /// Trivial and suppressed functions are excluded from the calculation.
    pub iosp_score: f64,
    /// Number of functions exceeding complexity thresholds.
    pub complexity_warnings: usize,
    /// Number of individual magic number occurrences across all functions.
    pub magic_number_warnings: usize,
    /// Number of functions exceeding nesting depth threshold.
    pub nesting_depth_warnings: usize,
    /// Number of functions exceeding function length threshold.
    pub function_length_warnings: usize,
    /// Number of functions containing unsafe blocks.
    pub unsafe_warnings: usize,
    /// Number of functions with error handling issues (unwrap/panic/todo).
    pub error_handling_warnings: usize,
    /// Number of modules with coupling warnings (exceeding thresholds).
    pub coupling_warnings: usize,
    /// Number of circular dependencies found.
    pub coupling_cycles: usize,
    /// Number of individual entries across all duplicate function groups.
    pub duplicate_groups: usize,
    /// Number of dead code warnings.
    pub dead_code_warnings: usize,
    /// Number of individual entries across all duplicate fragment groups.
    pub fragment_groups: usize,
    /// Number of boilerplate pattern findings.
    pub boilerplate_warnings: usize,
    /// Number of structs exceeding SRP smell threshold.
    pub srp_struct_warnings: usize,
    /// Number of modules exceeding production line thresholds.
    pub srp_module_warnings: usize,
    /// Number of functions with `#[allow(clippy::too_many_arguments)]`.
    pub srp_param_warnings: usize,
    /// Number of wildcard import warnings.
    pub wildcard_import_warnings: usize,
    /// Number of individual entries across all repeated match pattern groups (DRY-005).
    pub repeated_match_groups: usize,
    /// Number of Stable Dependencies Principle violations.
    pub sdp_violations: usize,
    /// Number of TQ-001 warnings: tests without assertions.
    pub tq_no_assertion_warnings: usize,
    /// Number of TQ-002 warnings: tests without SUT calls.
    pub tq_no_sut_warnings: usize,
    /// Number of TQ-003 warnings: untested production functions.
    pub tq_untested_warnings: usize,
    /// Number of TQ-004 warnings: uncovered production functions (LCOV).
    pub tq_uncovered_warnings: usize,
    /// Number of TQ-005 warnings: untested logic branches (LCOV).
    pub tq_untested_logic_warnings: usize,
    /// Number of structural binary check warnings mapped to SRP.
    pub structural_srp_warnings: usize,
    /// Number of structural binary check warnings mapped to Coupling.
    pub structural_coupling_warnings: usize,
    /// Number of Architecture-Dimension findings (layer/forbidden/pattern/trait_contract).
    pub architecture_warnings: usize,
    /// Weighted quality score across all dimensions (0.0–1.0).
    pub quality_score: f64,
    /// Per-dimension scores: [IOSP, Complexity, DRY, SRP, Coupling, TestQuality, Architecture].
    pub dimension_scores: [f64; 7],
    /// Total number of ALL allow suppressions: `// qual:allow` + `#[allow(...)]`.
    pub all_suppressions: usize,
    /// Whether the suppression ratio exceeds the configured maximum.
    pub suppression_ratio_exceeded: bool,
    /// Number of `// qual:allow(...)` markers that did not match any
    /// finding within their annotation window. Orphan markers are
    /// typically stale suppressions (the underlying finding was fixed
    /// or moved) or misplaced annotations.
    pub orphan_suppressions: usize,
}

impl Summary {
    pub fn from_results(results: &[FunctionAnalysis]) -> Self {
        let mut s = Self {
            total: results.len(),
            ..Default::default()
        };
        for r in results {
            if r.suppressed {
                s.suppressed += 1;
                continue;
            }
            match &r.classification {
                Classification::Integration => s.integrations += 1,
                Classification::Operation => s.operations += 1,
                Classification::Violation { .. } => s.violations += 1,
                Classification::Trivial => s.trivial += 1,
            }
        }
        // Score: ratio of compliant non-trivial functions
        let non_trivial = s.integrations + s.operations + s.violations;
        s.iosp_score = if non_trivial > 0 {
            (s.integrations + s.operations) as f64 / non_trivial as f64
        } else {
            1.0
        };
        s
    }

    /// Compute the overall quality score from all dimension findings.
    /// Operation: arithmetic logic on summary fields.
    pub fn compute_quality_score(&mut self, weights: &[f64; 7]) {
        let n = self.total.max(1) as f64;
        let complexity_count = self.complexity_warnings
            + self.magic_number_warnings
            + self.nesting_depth_warnings
            + self.function_length_warnings
            + self.unsafe_warnings
            + self.error_handling_warnings;
        let tq_count = self.tq_no_assertion_warnings
            + self.tq_no_sut_warnings
            + self.tq_untested_warnings
            + self.tq_uncovered_warnings
            + self.tq_untested_logic_warnings;
        self.dimension_scores = [
            self.iosp_score,
            1.0 - (complexity_count as f64 / n).min(1.0),
            1.0 - ((self.duplicate_groups
                + self.fragment_groups
                + self.dead_code_warnings
                + self.boilerplate_warnings
                + self.wildcard_import_warnings
                + self.repeated_match_groups) as f64
                / n)
                .min(1.0),
            1.0 - ((self.srp_struct_warnings
                + self.srp_module_warnings
                + self.srp_param_warnings
                + self.structural_srp_warnings) as f64
                / n)
                .min(1.0),
            1.0 - ((self.coupling_warnings
                + self.coupling_cycles * 2
                + self.sdp_violations
                + self.structural_coupling_warnings) as f64
                / n)
                .min(1.0),
            1.0 - (tq_count as f64 / n).min(1.0),
            1.0 - (self.architecture_warnings as f64 / n).min(1.0),
        ];
        // Scale by the number of active (non-zero weight) dimensions so the weighted-average
        // deficit is not diluted simply because the weights sum to 1.0 across multiple dimensions.
        // This preserves dimension weighting while making a given number of findings reduce
        // the overall score proportionally to the total function count.
        let active_dims = weights.iter().filter(|&&w| w > f64::EPSILON).count() as f64;
        let weighted_avg: f64 = self
            .dimension_scores
            .iter()
            .zip(weights.iter())
            .map(|(s, w)| s * w)
            .sum();
        let scale = if active_dims > 0.0 { active_dims } else { 1.0 };
        self.quality_score = (1.0 - scale * (1.0 - weighted_avg)).clamp(0.0, 1.0);
    }

    /// Total number of findings across all dimensions.
    /// Operation: arithmetic.
    pub fn total_findings(&self) -> usize {
        self.violations
            + self.complexity_warnings
            + self.magic_number_warnings
            + self.nesting_depth_warnings
            + self.function_length_warnings
            + self.unsafe_warnings
            + self.error_handling_warnings
            + self.duplicate_groups
            + self.fragment_groups
            + self.dead_code_warnings
            + self.boilerplate_warnings
            + self.srp_struct_warnings
            + self.srp_module_warnings
            + self.srp_param_warnings
            + self.wildcard_import_warnings
            + self.repeated_match_groups
            + self.coupling_warnings
            + self.coupling_cycles
            + self.sdp_violations
            + self.tq_no_assertion_warnings
            + self.tq_no_sut_warnings
            + self.tq_untested_warnings
            + self.tq_uncovered_warnings
            + self.tq_untested_logic_warnings
            + self.structural_srp_warnings
            + self.structural_coupling_warnings
            + self.architecture_warnings
            + self.orphan_suppressions
    }
}

#[cfg(test)]
mod tests;
