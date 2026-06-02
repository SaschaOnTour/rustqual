//! Boilerplate-detection (BP-001..BP-010 + config filtering) tests. Split
//! into focused sub-files (each ≤ the SRP file-length cap); the shared
//! `parse` helper + imports live here and reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::dry::boilerplate::*;
pub(super) use crate::config::sections::BoilerplateConfig;

mod bp001_to_005;
mod bp006_to_008;
mod bp009_onward;

pub(super) fn parse(code: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(code).expect("parse failed");
    vec![("test.rs".to_string(), code.to_string(), syntax)]
}
