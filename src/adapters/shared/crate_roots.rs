//! Which files Cargo compiles as a crate root — the single source of truth.
//!
//! A crate root is what makes a directory a *package*: the default
//! `src/lib.rs` / `src/main.rs`, and Cargo's two autobinary forms
//! `src/bin/<name>.rs` and `src/bin/<name>/main.rs`. Anything deeper under
//! `src/bin/` is a module of one of those binaries, not a root of its own.
//!
//! Two passes need this and must not disagree: cfg-test classification asks
//! *which directories are package roots* (so `<root>/tests/**` is an
//! integration test), external reachability asks *where a module tree starts*
//! and whether it is a library (only a library exposes anything outside).
//!
//! Derived from the parsed `.rs` set alone — no manifest, no filesystem. The
//! documented gap is a custom `[lib] path = …` / `[[bin]] path = …` layout,
//! which no path rule can detect. Callers pass forward-slash normalised paths.

/// The two kinds of crate root. They are distinguished because only a library
/// exposes items to consumers outside the crate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CrateRootKind {
    Lib,
    Bin,
}

/// If `path` is a crate root, the owning package directory (`""` for the
/// analysis-root crate) and which kind of root it is.
/// Integration: combines the library and binary lookups.
pub(crate) fn crate_root_of(path: &str) -> Option<(&str, CrateRootKind)> {
    owner_with_tail(path, "src/lib.rs")
        .map(|owner| (owner, CrateRootKind::Lib))
        .or_else(|| binary_root_owner(path).map(|owner| (owner, CrateRootKind::Bin)))
}

/// The owner of a binary root: the default `src/main.rs` or an autobinary.
/// Integration: two lookups, first match wins.
fn binary_root_owner(path: &str) -> Option<&str> {
    owner_with_tail(path, "src/main.rs").or_else(|| autobinary_owner(path))
}

/// Owner directory of an autobinary crate root under `src/bin/` (`""` for a
/// top-level one), or `None`.
/// Operation: prefix split + filter, own logic hidden in the closure.
fn autobinary_owner(path: &str) -> Option<&str> {
    path.strip_prefix("src/bin/")
        .map(|name| ("", name))
        .or_else(|| path.split_once("/src/bin/"))
        .filter(|(_, name)| is_autobinary_tail(name))
        .map(|(owner, _)| owner)
}

/// The part after `src/bin/` of Cargo's two autobinary forms: `<name>.rs` and
/// `<name>/main.rs`. `tools/helper.rs` is neither — it is a module of some
/// binary, and claiming it as a root would invent a package.
/// Operation: segment count + suffix tests, no own calls.
fn is_autobinary_tail(name: &str) -> bool {
    let depth = name.matches('/').count();
    (depth == 0 && name.ends_with(".rs")) || (depth == 1 && name.ends_with("/main.rs"))
}

/// Owner directory of a path ending in `<owner>/{tail}` (`""` when the path
/// equals `tail`), or `None`. Boundary-aware via the trailing `/`.
/// Operation: suffix matching, no own calls.
fn owner_with_tail<'a>(path: &'a str, tail: &str) -> Option<&'a str> {
    (path == tail)
        .then_some("")
        .or_else(|| path.strip_suffix(tail).and_then(|p| p.strip_suffix('/')))
}
