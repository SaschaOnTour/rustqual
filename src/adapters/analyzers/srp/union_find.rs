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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_singletons() {
        let mut uf = UnionFind::new(5);
        assert_eq!(uf.component_members().len(), 5);
        for i in 0..5 {
            assert_eq!(uf.find(i), i);
        }
    }

    #[test]
    fn test_union_merges_components() {
        let mut uf = UnionFind::new(4);
        uf.union(0, 1);
        uf.union(2, 3);
        assert_eq!(uf.component_members().len(), 2);
        assert_eq!(uf.find(0), uf.find(1));
        assert_eq!(uf.find(2), uf.find(3));
        assert_ne!(uf.find(0), uf.find(2));
    }

    #[test]
    fn test_union_transitive() {
        let mut uf = UnionFind::new(3);
        uf.union(0, 1);
        uf.union(1, 2);
        assert_eq!(uf.component_members().len(), 1);
        assert_eq!(uf.find(0), uf.find(2));
    }

    #[test]
    fn test_union_idempotent() {
        let mut uf = UnionFind::new(2);
        uf.union(0, 1);
        uf.union(0, 1);
        assert_eq!(uf.component_members().len(), 1);
    }

    #[test]
    fn test_empty() {
        let mut uf = UnionFind::new(0);
        assert_eq!(uf.component_members().len(), 0);
    }

    #[test]
    fn test_single_element() {
        let mut uf = UnionFind::new(1);
        assert_eq!(uf.component_members().len(), 1);
        assert_eq!(uf.find(0), 0);
    }

    #[test]
    fn test_component_members() {
        let mut uf = UnionFind::new(4);
        uf.union(0, 1);
        uf.union(2, 3);
        let members = uf.component_members();
        assert_eq!(members.len(), 2);
        // Each component should have exactly 2 elements
        for elems in members.values() {
            assert_eq!(elems.len(), 2);
        }
    }
}
