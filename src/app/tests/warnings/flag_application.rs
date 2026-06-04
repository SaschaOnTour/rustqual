//! `apply_file_suppressions` (IOSP/Complexity proximity window = 3) and
//! `apply_recursive_annotations` (drop self-calls). Fixtures place a
//! suppression at a window boundary / wrong dimension, and a recursive marker
//! over a function whose own-calls include its own name & qualified name.
use super::*;
use crate::adapters::analyzers::iosp::ComplexityMetrics;
use crate::findings::{Dimension, Suppression};

/// `(suppressed, complexity_suppressed)` for a function at `fa_line` with a
/// single suppression at `sup_line` covering `dim`.
fn file_suppressed(fa_line: usize, sup_line: usize, dim: Dimension) -> (bool, bool) {
    let mut fa = make_func_with_metrics(ComplexityMetrics::default());
    fa.line = fa_line;
    let sup = Suppression {
        line: sup_line,
        dimensions: vec![dim],
        reason: None,
    };
    apply_file_suppressions(&mut fa, &[sup]);
    (fa.suppressed, fa.complexity_suppressed)
}

#[test]
fn iosp_suppression_on_same_line_suppresses_iosp_only() {
    // An IOSP suppression in window marks `suppressed` (kills the outer `||`→`&&`
    // and the `is_adjacent && covers_iosp` conjunction) but NOT
    // `complexity_suppressed` (kills the complexity arm's `&&`→`||`).
    assert_eq!(file_suppressed(5, 5, Dimension::Iosp), (true, false));
}

#[test]
fn complexity_suppression_on_same_line_suppresses_complexity_only() {
    assert_eq!(file_suppressed(5, 5, Dimension::Complexity), (false, true));
}

#[test]
fn suppression_at_window_edge_applies() {
    // diff 3 == window → adjacent.
    assert_eq!(file_suppressed(5, 2, Dimension::Iosp), (true, false));
}

#[test]
fn suppression_just_outside_window_does_not_apply() {
    // fa.line 6, sup.line 2 → diff 4 > window 3 → not adjacent. Guards the
    // `fa.line - s.line` subtraction (a `/` gives 6/2=3 ≤ 3 and would apply).
    assert_eq!(file_suppressed(6, 2, Dimension::Iosp), (false, false));
}

#[test]
fn suppression_below_function_does_not_apply() {
    // sup.line 5 > fa.line 2 → not adjacent (and the `&&` short-circuits before
    // the subtraction underflows).
    assert_eq!(file_suppressed(2, 5, Dimension::Iosp), (false, false));
}

#[test]
fn recursive_annotation_drops_self_calls_only() {
    // A `qual:recursive`-marked fn (name `f`, qualified `Foo::f`) keeps only the
    // non-self call. Guards `call != self_name && call != qualified` against the
    // `!=`→`==` and `&&`→`||` mutations (each would keep a self-call).
    let mut fa = make_func_with_metrics(ComplexityMetrics::default());
    fa.name = "f".to_string();
    fa.qualified_name = "Foo::f".to_string();
    fa.line = 7;
    fa.own_calls = vec!["f".to_string(), "Foo::f".to_string(), "other".to_string()];
    let recursive_lines: HashMap<String, HashSet<usize>> =
        [("test.rs".to_string(), HashSet::from([7]))].into();
    apply_recursive_annotations(std::slice::from_mut(&mut fa), &recursive_lines);
    assert_eq!(fa.own_calls, vec!["other".to_string()]);
}
