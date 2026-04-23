//! Shared test helpers for Check A / Check B integration-style tests.

use crate::adapters::shared::use_tree::gather_alias_map;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashMap;

/// In-memory workspace built from `(path, source)` pairs.
pub(super) struct Workspace {
    pub files: Vec<(String, String, syn::File)>,
    pub aliases_per_file: HashMap<String, HashMap<String, Vec<String>>>,
}

pub(super) fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse")
}

pub(super) fn globset(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).unwrap());
    }
    b.build().unwrap()
}

/// Build a workspace + pre-compute alias maps per file.
pub(super) fn build_workspace(entries: &[(&str, &str)]) -> Workspace {
    let mut files = Vec::new();
    let mut aliases_per_file = HashMap::new();
    for (path, src) in entries {
        let ast = parse(src);
        let alias_map = gather_alias_map(&ast);
        aliases_per_file.insert(path.to_string(), alias_map);
        files.push((path.to_string(), src.to_string(), ast));
    }
    Workspace {
        files,
        aliases_per_file,
    }
}

/// Borrow the parsed files as `(&path, &syn::File)` — the shape the
/// graph + pub-fn collectors accept. Tied to `ws`'s lifetime.
pub(super) fn borrowed_files(ws: &Workspace) -> Vec<(&str, &syn::File)> {
    ws.files.iter().map(|(p, _, f)| (p.as_str(), f)).collect()
}
