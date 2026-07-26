//! External reachability: can an item be named from *outside* its crate?
//!
//! `qual:api` only means something for items an outside consumer can call.
//! An item behind a private `mod` is unreachable no matter how many `pub`
//! keywords it carries, so a `qual:api` there is a category error, not a
//! suppression. These tests pin the derivation (file → module path, `pub mod`
//! chain, inline modules, `pub use` re-exports) and its conservative bias:
//! when in doubt, call it reachable, so the marker is left alone.

pub(super) use crate::adapters::shared::reachability::{compute_external_reach, ExternalReach};

pub(super) fn parse(files: &[(&str, &str)]) -> Vec<(String, String, syn::File)> {
    files
        .iter()
        .map(|(path, src)| {
            (
                path.to_string(),
                src.to_string(),
                syn::parse_file(src).expect("fixture must parse"),
            )
        })
        .collect()
}

mod layout;
mod modules;
mod reexports;
