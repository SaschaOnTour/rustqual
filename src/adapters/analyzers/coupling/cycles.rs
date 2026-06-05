use super::{CycleReport, ModuleGraph};

/// Minimum SCC size to report as a cycle (single nodes are not cycles).
const MIN_CYCLE_SIZE: usize = 2;

/// Detect circular dependencies using Kosaraju's algorithm (iterative).
/// Integration: finish-order DFS, reverse graph, then SCC collection.
pub(super) fn detect_cycles(graph: &ModuleGraph) -> Vec<CycleReport> {
    if graph.modules.is_empty() {
        return vec![];
    }
    let order = finish_order(graph);
    let reverse = reverse_graph(graph);
    collect_sccs(graph, &order, &reverse)
}

/// Kosaraju pass 1: iterative DFS finish order over the forward graph.
/// Operation: per-root DFS via a helper.
fn finish_order(graph: &ModuleGraph) -> Vec<usize> {
    let n = graph.modules.len();
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for start in 0..n {
        if !visited[start] {
            dfs_finish(graph, start, &mut visited, &mut order);
        }
    }
    order
}

/// Iterative post-order DFS from `start`, appending nodes to `order` as they
/// finish.
/// Operation: explicit stack walk.
fn dfs_finish(graph: &ModuleGraph, start: usize, visited: &mut [bool], order: &mut Vec<usize>) {
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
            order.push(*node);
            stack.pop();
        }
    }
}

/// Kosaraju pass 2: the reverse adjacency of the forward graph.
/// Operation: edge transposition, no own calls.
fn reverse_graph(graph: &ModuleGraph) -> Vec<Vec<usize>> {
    let mut reverse = vec![vec![]; graph.modules.len()];
    for (from, neighbors) in graph.forward.iter().enumerate() {
        for &to in neighbors {
            reverse[to].push(from);
        }
    }
    reverse
}

/// Kosaraju pass 3: DFS the reverse graph in reverse finish order, reporting
/// each SCC of size `>= MIN_CYCLE_SIZE`.
/// Operation: per-root component collection via helpers.
fn collect_sccs(graph: &ModuleGraph, order: &[usize], reverse: &[Vec<usize>]) -> Vec<CycleReport> {
    let mut visited = vec![false; graph.modules.len()];
    let mut reports = Vec::new();
    for &start in order.iter().rev() {
        if visited[start] {
            continue;
        }
        let component = collect_component(start, reverse, &mut visited);
        if component.len() >= MIN_CYCLE_SIZE {
            reports.push(scc_report(graph, &component));
        }
    }
    reports
}

/// The reverse-graph component reachable from `start` (iterative DFS).
/// Operation: explicit stack walk.
fn collect_component(start: usize, reverse: &[Vec<usize>], visited: &mut [bool]) -> Vec<usize> {
    let mut component = Vec::new();
    let mut stack = vec![start];
    visited[start] = true;
    while let Some(node) = stack.pop() {
        component.push(node);
        for &next in &reverse[node] {
            if !visited[next] {
                visited[next] = true;
                stack.push(next);
            }
        }
    }
    component
}

/// A `CycleReport` carrying the SCC's module names, sorted.
/// Operation: name lookup + sort via closures.
fn scc_report(graph: &ModuleGraph, component: &[usize]) -> CycleReport {
    let mut names: Vec<String> = component
        .iter()
        .map(|&i| graph.modules[i].clone())
        .collect();
    names.sort();
    CycleReport { modules: names }
}
