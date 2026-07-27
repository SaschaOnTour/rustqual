//! `ProjectScope` / own-call recognition tests. Split into focused sub-files
//! (each ≤ the SRP file-length cap); shared imports + the `build_scope` helper
//! live here and reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::shared::project_scope::*;

mod collection_and_ownership;
mod trivial_and_edge_cases;

pub(super) fn build_scope(code: &str) -> ProjectScope {
    let syntax = syn::parse_file(code).expect("Failed to parse test code");
    let files = vec![("test.rs", &syntax)];
    ProjectScope::from_files(&files)
}
