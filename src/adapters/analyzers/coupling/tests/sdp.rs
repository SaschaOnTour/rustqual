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
fn sdp_flags_stable_module_depending_on_unstable_one_with_exact_instabilities() {
    // The canonical violation: stable `a` (Ca=2 via x,y; Ce=1 → b; I=1/3)
    // depends on unstable `b` (Ca=1 via a; Ce=2 → p,q; I=2/3). The edge points
    // stable → unstable, inverting SDP. Asserts the EXACT instabilities (not
    // just `< 0.5` / `> 0.5`) so a metric-formula regression is caught here.
    let graph = sdp_graph();
    let metrics = compute_coupling_metrics(&graph);
    let violations = check_sdp(&graph, &metrics);
    assert_eq!(violations.len(), 1, "exactly the a→b edge violates");
    let v = &violations[0];
    assert_eq!(v.from_module, "a");
    assert_eq!(v.to_module, "b");
    assert!(
        v.from_instability < v.to_instability,
        "stable from-module is less unstable than the to-module"
    );
    assert!(
        (v.from_instability - 1.0 / 3.0).abs() < 1e-9,
        "from-instability ≈ 1/3, got {}",
        v.from_instability
    );
    assert!(
        (v.to_instability - 2.0 / 3.0).abs() < 1e-9,
        "to-instability ≈ 2/3, got {}",
        v.to_instability
    );
}

#[test]
fn sdp_collects_every_violating_edge_not_just_the_first() {
    // One stable hub `a` (Ca=2 via x,y; Ce=2 → b,c; I=0.5) depends on TWO
    // unstable modules b and c (each Ca=1, Ce=2 → leaves; I=2/3). Both a→b and
    // a→c invert SDP, so the check must accumulate BOTH — not stop at the
    // first. Pins the loop's collect-all behaviour.
    let graph = ModuleGraph {
        modules: vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "x".into(),
            "y".into(),
            "p".into(),
            "q".into(),
            "r".into(),
            "s".into(),
        ],
        forward: vec![
            vec![1, 2], // a → b, c
            vec![5, 6], // b → p, q
            vec![7, 8], // c → r, s
            vec![0],    // x → a
            vec![0],    // y → a
            vec![],     // p
            vec![],     // q
            vec![],     // r
            vec![],     // s
        ],
    };
    let metrics = compute_coupling_metrics(&graph);
    let mut violations = check_sdp(&graph, &metrics);
    violations.sort_by(|l, r| l.to_module.cmp(&r.to_module));
    let edges: Vec<(&str, &str)> = violations
        .iter()
        .map(|v| (v.from_module.as_str(), v.to_module.as_str()))
        .collect();
    assert_eq!(edges, vec![("a", "b"), ("a", "c")], "both edges collected");
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

// (Former `test_violation_details` folded into
// `sdp_flags_stable_module_depending_on_unstable_one_with_exact_instabilities`
// above — same fixture (`sdp_graph()`), stronger oracle.)

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
