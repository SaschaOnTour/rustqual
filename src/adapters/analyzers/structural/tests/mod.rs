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
