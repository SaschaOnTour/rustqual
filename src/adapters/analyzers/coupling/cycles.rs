use super::{CycleReport, ModuleGraph};

/// Minimum SCC size to report as a cycle (single nodes are not cycles).
const MIN_CYCLE_SIZE: usize = 2;

/// Detect circular dependencies using Kosaraju's algorithm (iterative).
/// Operation: graph traversal logic (two-pass DFS), no own calls.
// qual:allow(complexity) reason: "Kosaraju three-pass algorithm is inherently complex"
pub(super) fn detect_cycles(graph: &ModuleGraph) -> Vec<CycleReport> {
    let n = graph.modules.len();
    if n == 0 {
        return vec![];
    }

    // Pass 1: iterative DFS to compute finish order
    let mut visited = vec![false; n];
    let mut finish_order = Vec::with_capacity(n);

    for start in 0..n {
        if visited[start] {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        visited[start] = true;

        while let Some((node, idx)) = stack.last_mut() {
            if *idx < graph.forward[*node].len() {
                let next = graph.forward[*node][*idx];
                *idx += 1;
                if !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                finish_order.push(*node);
                stack.pop();
            }
        }
    }

    // Pass 2: build reverse graph
    let mut reverse = vec![vec![]; n];
    for (from, neighbors) in graph.forward.iter().enumerate() {
        for &to in neighbors {
            reverse[to].push(from);
        }
    }

    // Pass 3: DFS on reverse graph in reverse finish order
    let mut visited2 = vec![false; n];
    let mut sccs = Vec::new();

    for &start in finish_order.iter().rev() {
        if visited2[start] {
            continue;
        }
        let mut component = Vec::new();
        let mut stack = vec![start];
        visited2[start] = true;

        while let Some(node) = stack.pop() {
            component.push(node);
            for &next in &reverse[node] {
                if !visited2[next] {
                    visited2[next] = true;
                    stack.push(next);
                }
            }
        }

        if component.len() >= MIN_CYCLE_SIZE {
            let mut names: Vec<String> = component
                .iter()
                .map(|&i| graph.modules[i].clone())
                .collect();
            names.sort();
            sccs.push(CycleReport { modules: names });
        }
    }

    sccs
}
