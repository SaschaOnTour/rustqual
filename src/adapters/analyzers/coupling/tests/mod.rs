mod integration;
mod root;
mod sdp;

/// Parse a source string into a `syn::File` for graph-building tests.
pub(super) fn parse_code(code: &str) -> syn::File {
    syn::parse_file(code).expect("Failed to parse test code")
}

/// Build the `(path, source, ast)` triples the coupling analyzer consumes.
pub(super) fn make_parsed(files: Vec<(&str, &str)>) -> Vec<(String, String, syn::File)> {
    files
        .into_iter()
        .map(|(path, code)| (path.to_string(), code.to_string(), parse_code(code)))
        .collect()
}
