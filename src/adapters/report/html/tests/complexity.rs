//! HTML complexity section: the flagged-key membership filter
//! (`matches_any_flagged`) and the `!suppressed && !complexity_suppressed`
//! row filter that decide which function rows are rendered.
use crate::report::html::complexity::format_complexity_section;
use crate::report::html::views::{
    HtmlComplexityDataView, HtmlComplexityFunctionRow, HtmlComplexityKey, HtmlComplexityView,
};

fn row(name: &str, line: usize, suppressed: bool) -> HtmlComplexityFunctionRow {
    HtmlComplexityFunctionRow {
        qualified_name: name.into(),
        file: "a.rs".into(),
        line,
        cognitive: 20,
        cyclomatic: 12,
        max_nesting: 5,
        function_lines: 80,
        issue_summary: "magic: 42".into(),
        suppressed,
        complexity_suppressed: false,
    }
}

#[test]
fn complexity_section_renders_only_flagged_unsuppressed_rows() {
    let finding_view = HtmlComplexityView {
        flagged_keys: vec![HtmlComplexityKey {
            file: "a.rs".into(),
            line: 5,
            is_magic_number: false,
        }],
    };
    let data_view = HtmlComplexityDataView {
        functions: vec![
            row("shown_fn", 5, false), // flagged (a.rs:5) + not suppressed → shown
            row("other_line_fn", 99, false), // a.rs:99 not in flagged keys → hidden
            row("suppressed_fn", 5, true), // flagged but suppressed → hidden
        ],
    };
    let html = format_complexity_section(&finding_view, &data_view);
    assert!(
        html.contains("Complexity \u{2014} 1 Warning"),
        "one warning: {html}"
    );
    assert!(
        html.contains("shown_fn"),
        "flagged unsuppressed row shown: {html}"
    );
    assert!(
        !html.contains("other_line_fn"),
        "non-flagged line excluded (matches_any_flagged file&&line): {html}"
    );
    assert!(
        !html.contains("suppressed_fn"),
        "suppressed row excluded: {html}"
    );
}

#[test]
fn complexity_magic_number_key_flags_whole_file_regardless_of_line() {
    // A magic-number key (is_magic_number = true) matches any line in the file.
    let finding_view = HtmlComplexityView {
        flagged_keys: vec![HtmlComplexityKey {
            file: "a.rs".into(),
            line: 1,
            is_magic_number: true,
        }],
    };
    let data_view = HtmlComplexityDataView {
        functions: vec![row("magic_fn", 42, false)],
    };
    let html = format_complexity_section(&finding_view, &data_view);
    assert!(
        html.contains("magic_fn"),
        "magic-number key flags the whole file (line ignored): {html}"
    );
}

fn metrics_with(
    unsafe_blocks: usize,
    unwrap_count: usize,
    magic: bool,
) -> crate::domain::analysis_data::ComplexityMetricsRecord {
    crate::domain::analysis_data::ComplexityMetricsRecord {
        cognitive_complexity: 0,
        cyclomatic_complexity: 0,
        max_nesting: 0,
        function_lines: 0,
        unsafe_blocks,
        unwrap_count,
        expect_count: 0,
        panic_count: 0,
        todo_count: 0,
        logic_count: 0,
        call_count: 0,
        hotspots: vec![],
        magic_numbers: if magic {
            vec![crate::domain::analysis_data::MagicNumberOccurrence {
                line: 7,
                value: "42".into(),
            }]
        } else {
            vec![]
        },
        logic_occurrences: vec![],
    }
}

fn frec_with(
    metrics: crate::domain::analysis_data::ComplexityMetricsRecord,
) -> crate::domain::analysis_data::FunctionRecord {
    crate::domain::analysis_data::FunctionRecord {
        name: "f".into(),
        file: "a.rs".into(),
        line: 1,
        qualified_name: "f".into(),
        parent_type: None,
        classification: crate::domain::analysis_data::FunctionClassification::Operation,
        severity: None,
        complexity: Some(metrics),
        parameter_count: 0,
        own_calls: vec![],
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
        suppressed: false,
        complexity_suppressed: false,
    }
}

#[test]
fn build_issue_summary_lists_magic_unsafe_and_error_handling() {
    use crate::report::html::complexity::build_complexity_data_view;
    let view = build_complexity_data_view(&[frec_with(metrics_with(2, 3, true))]);
    let summary = &view.functions[0].issue_summary;
    assert!(summary.contains("magic"), "magic numbers: {summary}");
    assert!(
        summary.contains("2 unsafe"),
        "unsafe blocks (`> 0`): {summary}"
    );
    assert!(
        summary.contains("3unwrap"),
        "error-handling count (`> 0` + `!empty`): {summary}"
    );
}

#[test]
fn build_issue_summary_em_dash_when_clean() {
    use crate::report::html::complexity::build_complexity_data_view;
    let view = build_complexity_data_view(&[frec_with(metrics_with(0, 0, false))]);
    assert_eq!(
        view.functions[0].issue_summary, "\u{2014}",
        "no issues → em-dash"
    );
}
