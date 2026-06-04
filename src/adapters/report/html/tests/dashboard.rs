//! HTML dashboard: the quality-score percentage (`* PERCENTAGE_MULTIPLIER`) and
//! the score→color thresholds (`>= 0.8` green, `>= 0.5` yellow).
use crate::ports::Reporter;
use crate::report::html::HtmlReporter;
use crate::report::{AnalysisResult, Summary};

fn render_dashboard(quality: f64, dimension_scores: [f64; 7]) -> String {
    let mut summary = Summary {
        total: 100,
        quality_score: quality,
        ..Default::default()
    };
    summary.dimension_scores = dimension_scores;
    let analysis = AnalysisResult {
        results: vec![],
        summary,
        findings: crate::domain::AnalysisFindings::default(),
        data: crate::domain::AnalysisData::default(),
    };
    HtmlReporter {
        summary: &analysis.summary,
    }
    .render(&analysis.findings, &analysis.data)
}

/// The score-badge's `style="background:<color>"` — the ONLY place `color()`'s
/// output is observable (a literal `color:#48bb78` cell appears elsewhere
/// unconditionally, so plain `contains("#48bb78")` is useless).
fn badge_background(quality: f64) -> String {
    let html = render_dashboard(quality, [1.0; 7]);
    let marker = "score-badge\" style=\"background:";
    let start = html.find(marker).expect("score badge element") + marker.len();
    html[start..start + 7].to_string()
}

#[test]
fn dashboard_quality_badge_color_by_threshold() {
    // The badge background is colored from the quality score: >= 0.8 green,
    // >= 0.5 yellow, else red. Each threshold boundary pins one `>=`.
    assert_eq!(badge_background(0.8), "#48bb78", "0.8 → green (>= 0.8)");
    assert_eq!(badge_background(0.5), "#ecc94b", "0.5 → yellow (>= 0.5)");
    assert_eq!(badge_background(0.3), "#f56565", "0.3 → red (else)");
}

#[test]
fn dashboard_score_percentage() {
    // quality 0.8 → "80.0%" pins `pct = v * PERCENTAGE_MULTIPLIER`.
    assert!(
        render_dashboard(0.8, [1.0; 7]).contains("80.0%"),
        "quality percentage (* MULTIPLIER)"
    );
}
