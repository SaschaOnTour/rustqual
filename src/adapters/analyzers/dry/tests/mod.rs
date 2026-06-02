mod dead_code;
mod fragments;
mod functions;
mod match_patterns;
mod root;
mod wildcards;

/// Three free functions (`func_a`/`func_b`/`func_c`) with identical
/// `let x = 1; let y = x + 2; let z = y * x;` bodies in separate files — the
/// shared 3-way-match fixture used by both the duplicate-function and the
/// fragment three-way tests.
pub(super) fn three_matching_funcs() -> Vec<(String, String, syn::File)> {
    ["a", "b", "c"]
        .iter()
        .map(|n| {
            let src = format!("fn func_{n}() {{ let x = 1; let y = x + 2; let z = y * x; }}");
            let ast = syn::parse_file(&src).expect("fixture must parse");
            (format!("{n}.rs"), src, ast)
        })
        .collect()
}
