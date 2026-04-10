use super::{CouplingMetrics, ModuleGraph};

/// Compute coupling metrics (Ca, Ce, Instability) for each module.
/// Operation: arithmetic and iteration logic, no own calls.
pub(super) fn compute_coupling_metrics(graph: &ModuleGraph) -> Vec<CouplingMetrics> {
    let n = graph.modules.len();
    let mut afferent_indices: Vec<Vec<usize>> = vec![Vec::new(); n];

    for (from, neighbors) in graph.forward.iter().enumerate() {
        for &to in neighbors {
            afferent_indices[to].push(from);
        }
    }

    (0..n)
        .map(|i| {
            let ca = afferent_indices[i].len();
            let ce = graph.forward[i].len();
            let instability = if ca + ce > 0 {
                ce as f64 / (ca + ce) as f64
            } else {
                0.0
            };
            let incoming: Vec<String> = afferent_indices[i]
                .iter()
                .map(|&idx| graph.modules[idx].clone())
                .collect();
            let mut outgoing: Vec<String> = graph.forward[i]
                .iter()
                .map(|&idx| graph.modules[idx].clone())
                .collect();
            outgoing.sort();
            CouplingMetrics {
                module_name: graph.modules[i].clone(),
                afferent: ca,
                efferent: ce,
                instability,
                incoming,
                outgoing,
                suppressed: false,
                warning: false,
            }
        })
        .collect()
}
