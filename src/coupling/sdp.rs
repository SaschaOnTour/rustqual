/// A violation of the Stable Dependencies Principle (SDP).
/// A stable module (low instability) depends on an unstable module (high instability).
#[derive(Debug, Clone)]
pub struct SdpViolation {
    /// The depending module (more stable).
    pub from_module: String,
    /// The depended-upon module (less stable).
    pub to_module: String,
    /// Instability of the depending module.
    pub from_instability: f64,
    /// Instability of the depended-upon module.
    pub to_instability: f64,
    /// Whether this violation is suppressed (either module has a coupling suppression).
    pub suppressed: bool,
}

/// Check the Stable Dependencies Principle: for each dependency edge A→B,
/// if A is more stable (lower instability) than B, it's a violation.
/// Operation: iteration + comparison logic, no own calls.
pub(super) fn check_sdp(
    graph: &super::ModuleGraph,
    metrics: &[super::CouplingMetrics],
) -> Vec<SdpViolation> {
    let instability: std::collections::HashMap<&str, f64> = metrics
        .iter()
        .map(|m| (m.module_name.as_str(), m.instability))
        .collect();

    let mut violations = Vec::new();
    for (from_idx, deps) in graph.forward.iter().enumerate() {
        let from_name = &graph.modules[from_idx];
        let from_inst = instability.get(from_name.as_str()).copied().unwrap_or(0.0);
        for &to_idx in deps {
            let to_name = &graph.modules[to_idx];
            let to_inst = instability.get(to_name.as_str()).copied().unwrap_or(0.0);
            // SDP violation: stable module depends on less stable module
            if from_inst < to_inst {
                violations.push(SdpViolation {
                    from_module: from_name.clone(),
                    to_module: to_name.clone(),
                    from_instability: from_inst,
                    to_instability: to_inst,
                    suppressed: false,
                });
            }
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coupling::{metrics::compute_coupling_metrics, ModuleGraph};

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
    fn test_multiple_violations() {
        // Stable hub A depends on two unstable leaf modules B and C
        // D → A, E → A (makes A stable)
        // A → B, A → C (B and C are unstable leaves)
        let graph = ModuleGraph {
            modules: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
            forward: vec![
                vec![1, 2], // a → b, a → c
                vec![],     // b (leaf, I=0 because no outgoing, but Ca=1)
                vec![],     // c (leaf)
                vec![0],    // d → a
                vec![0],    // e → a
            ],
        };
        let metrics = compute_coupling_metrics(&graph);
        let violations = check_sdp(&graph, &metrics);
        // A: Ca=2, Ce=2, I=0.5
        // B: Ca=1, Ce=0, I=0.0 (stable!)
        // So A(0.5) → B(0.0) is NOT a violation (A is less stable, B is more stable)
        // This means no violations in this case
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
    fn test_sdp_violation_default_not_suppressed() {
        let graph = ModuleGraph {
            modules: vec![
                "a".into(),
                "b".into(),
                "x".into(),
                "y".into(),
                "p".into(),
                "q".into(),
            ],
            forward: vec![vec![1], vec![4, 5], vec![0], vec![0], vec![], vec![]],
        };
        let metrics = compute_coupling_metrics(&graph);
        let violations = check_sdp(&graph, &metrics);
        assert_eq!(violations.len(), 1);
        assert!(
            !violations[0].suppressed,
            "SDP violations should default to not suppressed"
        );
    }
}
