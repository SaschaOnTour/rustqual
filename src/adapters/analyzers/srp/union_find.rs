/// Iterative Union-Find with path halving (find) and union-by-rank.
/// Operation: data structure with no external dependencies.
pub(super) struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    pub(super) fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    pub(super) fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // path halving
            x = self.parent[x];
        }
        x
    }

    /// Attach the tree with smaller rank under the one with larger rank.
    /// Ranks are equivalent to tree heights and stay small even under
    /// adversarial input (near-amortised inverse-Ackermann).
    pub(super) fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                // `u8` is plenty for union-by-rank (tree height grows
                // like log₂ N), but `saturating_add` removes the sharp
                // edge of a debug-build panic or release-build wrap if
                // someone ever exercises this with an adversarial N.
                self.rank[ra] = self.rank[ra].saturating_add(1);
            }
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
