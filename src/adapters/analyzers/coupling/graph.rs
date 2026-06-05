use std::collections::{HashMap, HashSet};

use crate::adapters::shared::file_to_module::file_to_module;

use super::ModuleGraph;

/// Build a module dependency graph from parsed files' `use crate::` statements.
/// Integration: collect modules, then add each file's intra-crate edges.
pub(super) fn build_module_graph(parsed: &[(String, String, syn::File)]) -> ModuleGraph {
    let (modules, index) = collect_modules(parsed);
    let mut forward = vec![HashSet::<usize>::new(); modules.len()];
    for (path, _, syntax) in parsed {
        let source_idx = index[&file_to_module(path)];
        add_edges(
            &mut forward[source_idx],
            source_idx,
            &crate_dep_modules(syntax),
            &index,
        );
    }
    ModuleGraph {
        modules,
        forward: sort_adjacency(forward),
    }
}

/// Sorted unique module names + their index map.
/// Operation: set build + sort, own call in closure.
fn collect_modules(
    parsed: &[(String, String, syn::File)],
) -> (Vec<String>, HashMap<String, usize>) {
    let mut modules: Vec<String> = parsed
        .iter()
        .map(|(path, _, _)| file_to_module(path))
        .collect::<HashSet<String>>()
        .into_iter()
        .collect();
    modules.sort();
    let index = modules
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), i))
        .collect();
    (modules, index)
}

/// The `crate::<module>` second-segment names imported by a file's `use` items.
/// Operation: filter_map over fully-expanded use paths in closures.
fn crate_dep_modules(syntax: &syn::File) -> Vec<String> {
    use_paths(syntax)
        .into_iter()
        .filter_map(|segs| {
            (segs.first().is_some_and(|s| s == "crate") && segs.len() >= 2).then(|| segs[1].clone())
        })
        .collect()
}

/// Fully-expanded path-segment lists for every terminal node of a file's `use`
/// trees, via an iterative stack walk (paths and groups fan out, names/renames/
/// globs terminate). Unknown future `syn` variants are skipped.
/// Operation: stack walk with per-variant handling.
fn use_paths(syntax: &syn::File) -> Vec<Vec<String>> {
    let mut stack: Vec<(&syn::UseTree, Vec<String>)> = syntax
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Use(u) => Some((&u.tree, Vec::new())),
            _ => None,
        })
        .collect();
    let mut paths = Vec::new();
    while let Some((tree, segments)) = stack.pop() {
        match tree {
            syn::UseTree::Path(p) => stack.push((&p.tree, pushed(&segments, &p.ident))),
            syn::UseTree::Group(g) => g
                .items
                .iter()
                .for_each(|sub| stack.push((sub, segments.clone()))),
            syn::UseTree::Name(name) => paths.push(pushed(&segments, &name.ident)),
            syn::UseTree::Rename(r) => paths.push(pushed(&segments, &r.ident)),
            syn::UseTree::Glob(_) => paths.push(segments),
        }
    }
    paths
}

/// `segments` with `ident` appended.
/// Operation: clone + push, no own calls.
fn pushed(segments: &[String], ident: &syn::Ident) -> Vec<String> {
    let mut s = segments.to_vec();
    s.push(ident.to_string());
    s
}

/// Add `source_idx → dep` edges for every known, non-self dependency module.
/// Operation: per-dep lookup, own calls in closures.
fn add_edges(
    row: &mut HashSet<usize>,
    source_idx: usize,
    deps: &[String],
    index: &HashMap<String, usize>,
) {
    deps.iter()
        .filter_map(|dep| index.get(dep.as_str()).copied())
        .filter(|&dep_idx| dep_idx != source_idx)
        .for_each(|dep_idx| {
            row.insert(dep_idx);
        });
}

/// Convert adjacency sets to sorted Vecs for deterministic output.
/// Operation: per-row sort in closures.
fn sort_adjacency(forward: Vec<HashSet<usize>>) -> Vec<Vec<usize>> {
    forward
        .into_iter()
        .map(|set| {
            let mut v: Vec<usize> = set.into_iter().collect();
            v.sort();
            v
        })
        .collect()
}
