mod btc;
mod deh;
mod iet;
mod nms;
mod oi;
mod root;
mod sit;
mod slm;

/// Shared parsing helpers for structural-check unit tests. All test
/// files under this module parse fixtures into the same
/// `(path, content, syn::File)` triple the production code sees; this
/// helper pair centralises the boilerplate.
pub(super) fn parse_single(source: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(source).expect("test source");
    vec![("test.rs".to_string(), source.to_string(), syntax)]
}

pub(super) fn parse_multi(sources: &[(&str, &str)]) -> Vec<(String, String, syn::File)> {
    sources
        .iter()
        .map(|(path, src)| {
            let syntax = syn::parse_file(src).expect("test source");
            ((*path).to_string(), (*src).to_string(), syntax)
        })
        .collect()
}

use super::{StructuralMetadata, StructuralWarning};
use crate::config::StructuralConfig;
use std::collections::HashSet;

/// Run a parsed-tree structural detector (deh/iet/slm/nms/btc shape) over a
/// single-file fixture, returning its warnings. Centralises the
/// `parse_single → config → cfg_test_files → detect → warnings` boilerplate.
pub(super) fn detect_single(
    source: &str,
    detect: impl Fn(
        &mut Vec<StructuralWarning>,
        &[(String, String, syn::File)],
        &StructuralConfig,
        &HashSet<String>,
    ),
) -> Vec<StructuralWarning> {
    let parsed = parse_single(source);
    let config = StructuralConfig::default();
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(&parsed);
    let mut warnings = Vec::new();
    detect(&mut warnings, &parsed, &config, &cfg_test_files);
    warnings
}

/// Run a metadata-based structural detector (sit/oi shape) over an
/// already-parsed fixture, returning its warnings. Shared by the single-file
/// (sit) and multi-file (oi) callers via the appropriate `parse_*` helper.
pub(super) fn detect_meta(
    parsed: &[(String, String, syn::File)],
    detect: impl Fn(&mut Vec<StructuralWarning>, &StructuralMetadata, &StructuralConfig),
) -> Vec<StructuralWarning> {
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(parsed);
    let meta = super::collect_metadata(parsed, &cfg_test_files);
    let config = StructuralConfig::default();
    let mut warnings = Vec::new();
    detect(&mut warnings, &meta, &config);
    warnings
}
