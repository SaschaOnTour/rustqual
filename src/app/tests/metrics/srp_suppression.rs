//! `mark_srp_suppressions` proximity window (SRP_STRUCT_PARAM = 5). Struct and
//! param warnings share the same `sup.line <= w.line && w.line - sup.line <=
//! WINDOW && sup.covers(srp)` logic; the cases below put a suppression at each
//! boundary so the `&&` / `-` / comparison mutations flip the verdict.
use super::*;
use crate::adapters::analyzers::srp::{ParamSrpWarning, SrpWarning};
use crate::findings::Dimension;
use std::collections::HashMap;

fn srp_sups(line: usize, dim: Dimension) -> HashMap<String, Vec<Suppression>> {
    [(
        "test.rs".to_string(),
        vec![Suppression {
            line,
            dimensions: vec![dim],
            reason: None,
            target: None,
        }],
    )]
    .into()
}

fn struct_warning(line: usize) -> SrpWarning {
    SrpWarning {
        struct_name: "S".to_string(),
        file: "test.rs".to_string(),
        line,
        lcom4: 2,
        field_count: 3,
        method_count: 4,
        fan_out: 1,
        composite_score: 0.9,
        clusters: vec![],
        suppressed: false,
    }
}

fn srp_struct_suppressed(w_line: usize, sup_line: usize, dim: Dimension) -> bool {
    let mut srp = make_srp();
    srp.struct_warnings.push(struct_warning(w_line));
    mark_srp_suppressions(Some(&mut srp), &srp_sups(sup_line, dim));
    srp.struct_warnings[0].suppressed
}

fn srp_param_suppressed(w_line: usize, sup_line: usize, dim: Dimension) -> bool {
    let mut srp = make_srp();
    srp.param_warnings.push(ParamSrpWarning {
        function_name: "f".to_string(),
        file: "test.rs".to_string(),
        line: w_line,
        parameter_count: 6,
        suppressed: false,
    });
    mark_srp_suppressions(Some(&mut srp), &srp_sups(sup_line, dim));
    srp.param_warnings[0].suppressed
}

/// `(warning_line, suppression_line, dimension, expected_suppressed)`.
const WINDOW_CASES: &[(usize, usize, Dimension, bool)] = &[
    (5, 5, Dimension::Srp, true),         // same line — in window
    (6, 1, Dimension::Srp, true),         // diff 5 == window
    (7, 1, Dimension::Srp, false),        // diff 6 > window
    (5, 5, Dimension::Complexity, false), // in window but wrong dimension
    (10, 2, Dimension::Srp, false),       // diff 8 > window (kills `-`→`/`: 10/2=5≤5)
    (2, 5, Dimension::Srp, false),        // suppression below the warning
];

#[test]
fn srp_struct_suppression_respects_window_and_dimension() {
    for &(w, sup, dim, exp) in WINDOW_CASES {
        assert_eq!(
            srp_struct_suppressed(w, sup, dim),
            exp,
            "struct w={w} sup={sup} dim={dim:?}"
        );
    }
}

#[test]
fn srp_param_suppression_respects_window_and_dimension() {
    for &(w, sup, dim, exp) in WINDOW_CASES {
        assert_eq!(
            srp_param_suppressed(w, sup, dim),
            exp,
            "param w={w} sup={sup} dim={dim:?}"
        );
    }
}
