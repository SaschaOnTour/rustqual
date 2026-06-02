use crate::adapters::analyzers::coupling::sdp::*;
use crate::adapters::analyzers::coupling::{metrics::compute_coupling_metrics, ModuleGraph};

/// A 6-module graph with exactly one SDP violation (stable `a`→`b` where `b`
/// depends on the more-unstable `x`/`y`). The single `ModuleGraph` literal in
/// this module (keeping it out of BP-009's struct-update window).
fn sdp_graph() -> ModuleGraph {
    ModuleGraph {
        modules: vec![
            "a".into(),
            "b".into(),
            "x".into(),
            "y".into(),
            "p".into(),
            "q".into(),
        ],
        forward: vec![vec![1], vec![4, 5], vec![0], vec![0], vec![], vec![]],
    }
}

/// Run SDP on `sdp_graph()`, optionally pre-suppressing the metric at
/// `suppress_idx`, and report whether the (single) violation is suppressed.
fn sdp_first_violation_suppressed(suppress_idx: Option<usize>) -> bool {
    let graph = sdp_graph();
    let mut metrics = compute_coupling_metrics(&graph);
    if let Some(i) = suppress_idx {
        metrics[i].suppressed = true;
    }
    let violations = check_sdp(&graph, &metrics);
    assert_eq!(
        violations.len(),
        1,
        "fixture must have exactly one violation"
    );
    violations[0].suppressed
}

#[test]
fn test_no_violations_all_same_instability() {
    // A → B, both have same structure → no SDP violation
    let graph = ModuleGraph {
        modules: vec!["a".into(), "b".into()],
        forward: vec![vec![1], vec![0]],
    };
    let metrics = compute_coupling_metrics(&graph);
    let violations = check_sdp(&graph, &metrics);
    assert!(
        violations.is_empty(),
        "Equal instability should not trigger SDP"
    );
}

#[test]
fn test_violation_stable_depends_on_unstable() {
    // A(stable) → B(unstable), C → A (makes A stable)
    // C → A → B
    // A: Ca=1, Ce=1, I=0.5
    // B: Ca=1, Ce=0, I=0.0
    // C: Ca=0, Ce=1, I=1.0
    // Edge C→A: C(1.0) → A(0.5) — C is unstable, depends on more stable A → no violation
    // Edge A→B: A(0.5) → B(0.0) — A depends on more stable B → no violation
    // No violations here. Let me construct a case where there IS a violation.

    // A: Ca=2, Ce=0, I=0.0 (very stable)
    // B: Ca=0, Ce=2, I=1.0 (very unstable)
    // A → B would be an SDP violation
    // We need: X → A, Y → A (gives A Ca=2)
    //          B → P, B → Q (gives B Ce=2)
    //          A → B (the violating edge)
    let graph = ModuleGraph {
        modules: vec![
            "a".into(),
            "b".into(),
            "x".into(),
            "y".into(),
            "p".into(),
            "q".into(),
        ],
        forward: vec![
            vec![1],    // a → b
            vec![4, 5], // b → p, b → q
            vec![0],    // x → a
            vec![0],    // y → a
            vec![],     // p
            vec![],     // q
        ],
    };
    let metrics = compute_coupling_metrics(&graph);
    let violations = check_sdp(&graph, &metrics);
    // A: Ca=2, Ce=1, I=1/3 ≈ 0.33
    // B: Ca=1, Ce=2, I=2/3 ≈ 0.67
    // A → B: 0.33 < 0.67 → violation
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].from_module, "a");
    assert_eq!(violations[0].to_module, "b");
    assert!(violations[0].from_instability < violations[0].to_instability);
}

#[test]
fn test_no_violation_unstable_depends_on_stable() {
    // A(unstable) → B(stable) — this is correct per SDP
    // A: Ca=0, Ce=1, I=1.0
    // B: Ca=1, Ce=0, I=0.0
    let graph = ModuleGraph {
        modules: vec!["a".into(), "b".into()],
        forward: vec![vec![1], vec![]],
    };
    let metrics = compute_coupling_metrics(&graph);
    let violations = check_sdp(&graph, &metrics);
    assert!(
        violations.is_empty(),
        "Unstable depending on stable is correct SDP"
    );
}

#[test]
fn test_no_violations_empty_graph() {
    let graph = ModuleGraph {
        modules: vec![],
        forward: vec![],
    };
    let violations = check_sdp(&graph, &[]);
    assert!(violations.is_empty());
}

#[test]
fn test_no_violations_single_module() {
    let graph = ModuleGraph {
        modules: vec!["a".into()],
        forward: vec![vec![]],
    };
    let metrics = compute_coupling_metrics(&graph);
    let violations = check_sdp(&graph, &metrics);
    assert!(violations.is_empty());
}

#[test]
fn test_zero_violations_for_stable_leaves() {
    // Edge-case regression: the label "unstable leaf" is a common
    // intuition trap. B and C have Ce=0 and Ca=1 → instability = 0.0
    // (maximally stable). A → B is therefore NOT an SDP violation even
    // though the test name suggests multiple. The assertion locks the
    // correct "zero violations" behaviour so future refactors can't
    // silently change it.
    let graph = ModuleGraph {
        modules: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
        forward: vec![
            vec![1, 2], // a → b, a → c
            vec![],     // b (Ca=1, Ce=0 → I=0.0, stable)
            vec![],     // c (Ca=1, Ce=0 → I=0.0, stable)
            vec![0],    // d → a
            vec![0],    // e → a
        ],
    };
    let metrics = compute_coupling_metrics(&graph);
    let violations = check_sdp(&graph, &metrics);
    // A: Ca=2, Ce=2, I=0.5
    // B: Ca=1, Ce=0, I=0.0 (stable)
    // A (0.5) → B (0.0) is not a violation: the callee is more stable.
    assert!(violations.is_empty());
}

#[test]
fn test_violation_details() {
    // Make a clear violation: stable A depends on unstable B
    // Setup: X→A, Y→A gives A high Ca
    // B→P, B→Q gives B high Ce
    // A→B is the violation
    let graph = ModuleGraph {
        modules: vec![
            "a".into(),
            "b".into(),
            "x".into(),
            "y".into(),
            "p".into(),
            "q".into(),
        ],
        forward: vec![
            vec![1],    // a → b
            vec![4, 5], // b → p, q
            vec![0],    // x → a
            vec![0],    // y → a
            vec![],     // p
            vec![],     // q
        ],
    };
    let metrics = compute_coupling_metrics(&graph);
    let violations = check_sdp(&graph, &metrics);

    assert_eq!(violations.len(), 1);
    let v = &violations[0];
    assert_eq!(v.from_module, "a");
    assert_eq!(v.to_module, "b");
    // A: Ca=2, Ce=1, I≈0.33
    // B: Ca=1, Ce=2, I≈0.67
    assert!(v.from_instability < 0.5);
    assert!(v.to_instability > 0.5);
}

#[test]
fn sdp_violation_inherits_suppression_from_either_endpoint() {
    // A violation defaults to not-suppressed, but is created suppressed when
    // EITHER the from-module (`a`, idx 0) or the to-module (`b`, idx 1) is
    // suppressed. (label, suppressed_metric_idx, violation_suppressed)
    let cases: &[(&str, Option<usize>, bool)] = &[
        ("no module suppressed → not suppressed", None, false),
        ("from-module (a) suppressed", Some(0), true),
        ("to-module (b) suppressed", Some(1), true),
    ];
    for (label, suppress_idx, expected) in cases {
        assert_eq!(
            sdp_first_violation_suppressed(*suppress_idx),
            *expected,
            "case {label}"
        );
    }
}
