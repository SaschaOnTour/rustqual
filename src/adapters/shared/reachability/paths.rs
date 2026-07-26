//! Where a source file sits in its crate: package prefix, module path, and
//! the index key built from both. One place decides what "inside a package"
//! means, so the prefix and the module path can never disagree.

/// Split a source path at its package's `src/` into `(package prefix, path
/// below src/)`. `None` when the layout is not recognised.
/// Operation: path splitting, no own calls.
fn split_at_src(file: &str) -> Option<(String, String)> {
    let norm = file.replace('\\', "/");
    let idx = norm.find("src/")?;
    // Only `src/` at the start or right after a directory separator.
    if idx != 0 && !norm[..idx].ends_with('/') {
        return None;
    }
    Some((norm[..idx].to_string(), norm[idx + 4..].to_string()))
}

/// A module's index key: package prefix plus module path, so
/// `crates/a/src/api.rs` and `crates/b/src/api.rs` stay distinct.
/// Integration: package lookup + join.
pub(super) fn module_key(file: &str, path: &[String]) -> Option<String> {
    let (package, _) = split_at_src(file)?;
    Some(format!("{package}|{}", path.join("::")))
}

/// The module path of a source file relative to its package's `src/`
/// (`src/a/b.rs` → `["a","b"]`, `src/a/mod.rs` → `["a"]`, root → `[]`), or
/// `None` when the layout is not recognised — those files are treated as
/// reachable so an unusual layout never manufactures a finding.
/// Operation: segment derivation, no own calls beyond the splitter.
pub(super) fn module_path_of(file: &str) -> Option<Vec<String>> {
    let (_, below) = split_at_src(file)?;
    let rel = below.strip_suffix(".rs")?;
    let mut segments: Vec<String> = rel.split('/').map(str::to_string).collect();
    match segments.last().map(String::as_str) {
        Some("lib") | Some("main") if segments.len() == 1 => return Some(Vec::new()),
        Some("mod") => {
            segments.pop();
        }
        _ => {}
    }
    Some(segments)
}

/// True when `file` is a library crate root (`…/src/lib.rs`). A binary has no
/// outside consumers, so its items are never externally reachable.
/// Operation: suffix test, no own calls.
pub(super) fn is_lib_root(file: &str) -> bool {
    let norm = file.replace('\\', "/");
    norm == "src/lib.rs" || norm.ends_with("/src/lib.rs")
}

/// True for `pub` exactly — `pub(crate)` / `pub(super)` cannot leave the crate.
/// Operation: visibility match, no own calls.
pub(super) fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}
