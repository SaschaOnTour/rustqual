//! Rendering tests for the verbose per-file listing (`format_files_section`).
//! Asserting on the rendered string pins the file-visibility filter, the
//! verbose match guards, the violation detail lines, and the complexity /
//! warning formatting that the existing smoke tests don't reach.
use crate::adapters::analyzers::iosp::{
    Classification, ComplexityMetrics, FunctionAnalysis, MagicNumberOccurrence, Severity,
};
use crate::adapters::report::test_support::{make_result, violation};
use crate::report::text::files::format_files_section;

/// An unsuppressed Violation `v` with an effort score and High severity.
fn violation_fn() -> FunctionAnalysis {
    let mut f = make_result("v", violation("if", "helper"));
    f.severity = Some(Severity::High);
    f.effort_score = Some(3.5);
    f
}

/// An Operation with full complexity metrics and every complexity warning set.
fn warned_operation() -> FunctionAnalysis {
    let mut f = make_result("op", Classification::Operation);
    f.complexity = Some(ComplexityMetrics {
        logic_count: 3,
        call_count: 0,
        max_nesting: 0,
        cognitive_complexity: 20,
        cyclomatic_complexity: 12,
        function_lines: 80,
        unsafe_blocks: 2,
        unwrap_count: 2,
        magic_numbers: vec![MagicNumberOccurrence {
            line: 4,
            value: "42".into(),
        }],
        ..Default::default()
    });
    f.cognitive_warning = true;
    f.nesting_depth_warning = true;
    f.function_length_warning = true;
    f.unsafe_warning = true;
    f.error_handling_warning = true;
    f
}

#[test]
fn violation_entry_shows_logic_calls_effort_and_severity() {
    // A file with one unsuppressed violation is shown even non-verbose, with its
    // logic/call locations and effort. Pins has_violations and the
    // `!logic_locations.is_empty()` / `!call_locations.is_empty()` guards.
    let out = format_files_section(&[violation_fn()], false);
    assert!(out.contains("VIOLATION"), "{out}");
    assert!(out.contains("[HIGH]"), "severity tag: {out}");
    assert!(
        out.contains("Logic:") && out.contains("if"),
        "logic line: {out}"
    );
    assert!(
        out.contains("Calls:") && out.contains("helper"),
        "calls line: {out}"
    );
    assert!(
        out.contains("Effort:") && out.contains("3.5"),
        "effort line: {out}"
    );
}

#[test]
fn non_verbose_hides_clean_files_but_verbose_shows_them() {
    // A clean (unsuppressed, non-violation) Operation file is skipped without
    // --verbose and shown with it. Pins the `!verbose && !has_violations &&
    // !has_suppressed` skip and the per-arm `verbose` match guards.
    let op = make_result("clean_op", Classification::Operation);
    assert!(
        format_files_section(std::slice::from_ref(&op), false).is_empty(),
        "clean file hidden without --verbose"
    );
    let verbose = format_files_section(&[op], true);
    assert!(
        verbose.contains("OPERATION"),
        "shown with --verbose: {verbose}"
    );
    assert!(verbose.contains("clean_op"), "{verbose}");
}

#[test]
fn suppressed_function_keeps_its_file_visible() {
    // A file whose only function is suppressed is still shown (has_suppressed),
    // pinning the `!has_suppressed` term of the skip condition.
    let mut f = make_result("hidden", Classification::Operation);
    f.suppressed = true;
    let out = format_files_section(&[f], false);
    assert!(
        out.contains("test.rs"),
        "suppressed-only file header shown: {out}"
    );
}

#[test]
fn complexity_details_and_warnings_render() {
    // Verbose render of an Operation with metrics + warnings. Pins the
    // `logic>0 || call>0 || nesting>0` complexity-line guard, the magic-number
    // emptiness check, the unsafe singular/plural `== 1`, and the error-handling
    // `count > 0` filter.
    let out = format_files_section(&[warned_operation()], true);
    assert!(out.contains("logic=3"), "complexity line: {out}");
    assert!(out.contains("cognitive complexity 20"), "{out}");
    assert!(out.contains("magic numbers:"), "magic numbers msg: {out}");
    assert!(
        out.contains("2 unsafe blocks"),
        "plural unsafe (== 1 → else 's'): {out}"
    );
    assert!(
        out.contains("error handling:") && out.contains("2 unwrap"),
        "error msg: {out}"
    );
    assert!(out.contains("nesting depth"), "{out}");
    assert!(out.contains("function length 80 lines"), "{out}");
}

#[test]
fn complexity_line_hidden_when_all_metrics_zero() {
    // With logic/call/nesting all zero the "Complexity:" detail line is not
    // emitted. Pins the `> 0` comparisons against `==`/`<` (which would emit it).
    let mut f = make_result("zero", Classification::Operation);
    f.complexity = Some(ComplexityMetrics::default());
    let out = format_files_section(&[f], true);
    assert!(out.contains("OPERATION"), "{out}");
    assert!(
        !out.contains("logic="),
        "no complexity line when all zero: {out}"
    );
}

#[test]
fn non_verbose_violation_file_hides_non_violation_entries() {
    // A file shown because of a violation still hides its Integration/Operation/
    // Trivial functions when non-verbose (each arm is `verbose`-guarded).
    let out = format_files_section(
        &[
            violation_fn(),
            make_result("integ_fn", Classification::Integration),
            make_result("op_fn", Classification::Operation),
            make_result("triv_fn", Classification::Trivial),
        ],
        false,
    );
    assert!(out.contains("VIOLATION"), "violation shown: {out}");
    for hidden in ["integ_fn", "op_fn", "triv_fn"] {
        assert!(
            !out.contains(hidden),
            "{hidden} hidden non-verbose (pins its `verbose` match guard): {out}"
        );
    }
}

fn op_with_metrics(name: &str, m: ComplexityMetrics) -> FunctionAnalysis {
    let mut f = make_result(name, Classification::Operation);
    f.complexity = Some(m);
    f
}

#[test]
fn complexity_line_shown_for_calls_or_nesting_alone() {
    // The "Complexity:" line shows if ANY of logic/calls/nesting is > 0. Isolating
    // calls (then nesting) pins each `> 0` against `<` and the `||` chain.
    let calls_only = op_with_metrics(
        "calls_fn",
        ComplexityMetrics {
            call_count: 2,
            ..Default::default()
        },
    );
    assert!(
        format_files_section(&[calls_only], true).contains("calls=2"),
        "calls-only → complexity line shown"
    );
    let nesting_only = op_with_metrics(
        "nest_fn",
        ComplexityMetrics {
            max_nesting: 2,
            ..Default::default()
        },
    );
    assert!(
        format_files_section(&[nesting_only], true).contains("nesting=2"),
        "nesting-only → complexity line shown"
    );
}

#[test]
fn error_handling_lists_only_nonzero_counts() {
    // Only non-zero error-handling counts appear (pins `*count > 0` vs `>=`).
    let f = op_with_metrics(
        "err_fn",
        ComplexityMetrics {
            unwrap_count: 2,
            ..Default::default()
        },
    );
    let mut f = f;
    f.error_handling_warning = true;
    let out = format_files_section(&[f], true);
    assert!(out.contains("2 unwrap"), "unwrap listed: {out}");
    assert!(!out.contains("0 expect"), "zero counts not listed: {out}");
}
