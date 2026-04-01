use std::collections::{HashMap, HashSet};

use super::ModuleGraph;

/// Build a module dependency graph from parsed files' `use crate::` statements.
/// Operation: iterates files and use trees (stack-based), builds adjacency lists.
/// Calls `file_to_module` via closure for IOSP compliance.
// qual:allow(complexity) reason: "stack-based use-tree traversal requires complex loop"
pub(super) fn build_module_graph(parsed: &[(String, String, syn::File)]) -> ModuleGraph {
    let to_module = |path: &str| super::file_to_module(path);

    // Collect all unique module names
    let mut module_set: HashSet<String> = HashSet::new();
    for (path, _, _) in parsed {
        module_set.insert(to_module(path));
    }
    let mut modules: Vec<String> = module_set.into_iter().collect();
    modules.sort();

    let module_index: HashMap<String, usize> = modules
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), i))
        .collect();

    let n = modules.len();
    let mut forward = vec![HashSet::<usize>::new(); n];

    for (path, _, syntax) in parsed {
        let source_module = to_module(path);
        let source_idx = module_index[&source_module];

        // Stack-based iterative walk of use trees
        let mut stack: Vec<(&syn::UseTree, Vec<String>)> = Vec::new();
        for item in &syntax.items {
            if let syn::Item::Use(use_item) = item {
                stack.push((&use_item.tree, Vec::new()));
            }
        }

        while let Some((tree, segments)) = stack.pop() {
            match tree {
                syn::UseTree::Path(p) => {
                    let mut new_segments = segments.clone();
                    new_segments.push(p.ident.to_string());
                    stack.push((&p.tree, new_segments));
                    continue;
                }
                syn::UseTree::Group(g) => {
                    for subtree in &g.items {
                        stack.push((subtree, segments.clone()));
                    }
                    continue;
                }
                _ => {} // Name, Rename, Glob handled below
            }

            // Terminal nodes: compute final path segments
            let final_segments = match tree {
                syn::UseTree::Name(name) => {
                    let mut s = segments;
                    s.push(name.ident.to_string());
                    s
                }
                syn::UseTree::Rename(rename) => {
                    let mut s = segments;
                    s.push(rename.ident.to_string());
                    s
                }
                syn::UseTree::Glob(_) => segments,
                _ => unreachable!(),
            };

            // Only track intra-crate dependencies (use crate::xxx)
            if final_segments.first().is_some_and(|s| s == "crate") && final_segments.len() >= 2 {
                let dep_module = &final_segments[1];
                if let Some(&dep_idx) = module_index.get(dep_module.as_str()) {
                    if dep_idx != source_idx {
                        forward[source_idx].insert(dep_idx);
                    }
                }
            }
        }
    }

    // Convert HashSets to sorted Vecs for deterministic output
    let forward: Vec<Vec<usize>> = forward
        .into_iter()
        .map(|set| {
            let mut v: Vec<usize> = set.into_iter().collect();
            v.sort();
            v
        })
        .collect();

    ModuleGraph { modules, forward }
}
