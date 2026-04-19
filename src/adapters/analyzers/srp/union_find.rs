/// Iterative Union-Find with path halving.
/// Operation: data structure with no external dependencies.
pub(super) struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    pub(super) fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
        }
    }

    pub(super) fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // path halving
            x = self.parent[x];
        }
        x
    }

    pub(super) fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[ra] = rb;
        }
    }

    pub(super) fn component_members(&mut self) -> std::collections::HashMap<usize, Vec<usize>> {
        let n = self.parent.len();
        let mut components = std::collections::HashMap::new();
        for i in 0..n {
            let root = self.find(i);
            components.entry(root).or_insert_with(Vec::new).push(i);
        }
        components
    }
}
