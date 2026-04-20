//! Forbidden Rule — paired glob prohibition on cross-module imports.
//!
//! Each rule has a `from` file-path glob and a `to` file-path glob. A file
//! whose path matches `from` must not import anything that resolves to a
//! file-path matching `to`, unless that candidate also matches one of the
//! `except` globs.
//!
//! Imports are resolved by synthesising candidate file paths from a
//! crate-absolute segment list. `crate::a::b` resolves directly; `self`
//! and `super` are normalised against the importing file's own module
//! path (so `super::dry::helper` from `src/adapters/analyzers/iosp/…`
//! becomes `adapters::analyzers::dry::helper` before matching). At every
//! prefix length we consider both the leaf-as-file
//! (`src/<seg1>/…/<segN>.rs`) and leaf-as-dir
//! (`src/<seg1>/…/<segN>/mod.rs`) layouts. Imports starting with `std`,
//! `core`, `alloc`, or an external crate name are skipped — they have
//! no crate-relative file path, and other architecture rules cover
//! external crates.

#![cfg_attr(test, allow(dead_code))]

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::adapters::shared::use_tree::gather_imports;
use globset::{GlobMatcher, GlobSet};

/// Pre-compiled rule ready for checking.
#[derive(Debug)]
pub struct CompiledForbiddenRule {
    pub from: GlobMatcher,
    pub to: GlobMatcher,
    pub except: GlobSet,
    pub reason: String,
}

/// Check every file/rule pair.
/// Integration: per-file iteration + flat-map of per-file hits.
// qual:api
pub fn check_forbidden_rules(
    files: &[(String, &syn::File)],
    rules: &[CompiledForbiddenRule],
) -> Vec<MatchLocation> {
    files
        .iter()
        .flat_map(|(path, ast)| file_hits(path, ast, rules))
        .collect()
}

/// Collect every hit for one file across all applicable rules.
/// Operation: iterator chain over applicable rules × imports.
fn file_hits(path: &str, ast: &syn::File, rules: &[CompiledForbiddenRule]) -> Vec<MatchLocation> {
    let imports = gather_imports(ast);
    rules
        .iter()
        .filter(|r| r.from.is_match(path))
        .flat_map(|r| {
            imports
                .iter()
                .filter_map(|(segments, span)| evaluate_import(path, segments, *span, r))
        })
        .collect()
}

/// Evaluate one import against one rule; return a hit iff `to` matches a
/// candidate path and no `except` glob matches any candidate.
/// Operation: candidate construction + glob matching.
fn evaluate_import(
    path: &str,
    segments: &[String],
    span: proc_macro2::Span,
    rule: &CompiledForbiddenRule,
) -> Option<MatchLocation> {
    let inner = resolve_to_crate_absolute(path, segments)?;
    let candidates = candidate_paths(&inner);
    let to_hits = candidates.iter().any(|c| rule.to.is_match(c));
    if !to_hits {
        return None;
    }
    let except_hits = candidates.iter().any(|c| rule.except.is_match(c));
    if except_hits {
        return None;
    }
    let start = span.start();
    Some(MatchLocation {
        file: path.to_string(),
        line: start.line,
        column: start.column,
        kind: ViolationKind::ForbiddenEdge {
            reason: rule.reason.clone(),
            imported_path: segments.join("::"),
        },
    })
}

/// Resolve an import's segment list to its crate-absolute form.
/// `crate::a::b` → `["a","b"]`. `self::x` / `super[::super]*::x` are
/// normalised against the importing file's module path so the resolver
/// sees the same segment list regardless of import style. Returns
/// `None` for stdlib (`std`, `core`, `alloc`), external-crate imports,
/// or resolved paths that still contain a wildcard `*` segment (e.g.
/// `use crate::foo::*;`) — those cannot be turned into concrete
/// candidate file paths.
/// Operation: first-segment routing + path arithmetic, no own calls.
fn resolve_to_crate_absolute(importing_file: &str, segments: &[String]) -> Option<Vec<String>> {
    let first = segments.first()?;
    let resolved = match first.as_str() {
        "crate" => segments[1..].to_vec(),
        "self" => {
            let mut base = file_to_module_segments(importing_file);
            base.extend_from_slice(&segments[1..]);
            base
        }
        "super" => {
            let mut base = file_to_module_segments(importing_file);
            let mut i = 0;
            while segments.get(i).is_some_and(|s| s == "super") {
                // More `super`s than ancestors → silently ignore (no
                // architecture-rule meaning we can derive).
                base.pop()?;
                i += 1;
            }
            base.extend_from_slice(&segments[i..]);
            base
        }
        _ => return None,
    };
    // A resolved path with a `*` leaf (e.g. `crate::foo::*`) matches no
    // concrete file — skip so we don't emit bogus `src/*/…` candidates
    // that could collide with broad `to = "src/**"` rules.
    if resolved.iter().any(|s| s == "*") {
        return None;
    }
    Some(resolved)
}

/// Convert a file path under `src/` to its crate-absolute module
/// segment list. `src/lib.rs` / `src/main.rs` → `[]` (crate root);
/// `src/foo.rs` → `["foo"]`; `src/foo/mod.rs` → `["foo"]`;
/// `src/foo/bar.rs` → `["foo","bar"]`.
/// Operation: path-component parsing, no own calls.
fn file_to_module_segments(path: &str) -> Vec<String> {
    let normalised = path.replace('\\', "/");
    let stripped = normalised.strip_prefix("src/").unwrap_or(&normalised);
    let without_ext = stripped.strip_suffix(".rs").unwrap_or(stripped);
    if without_ext == "lib" || without_ext == "main" {
        return Vec::new();
    }
    let mut parts: Vec<String> = without_ext.split('/').map(String::from).collect();
    if parts.last().is_some_and(|s| s == "mod") {
        parts.pop();
    }
    parts
}

/// Synthesise the candidate `src/…` file paths for a segment prefix (the
/// `crate::` already stripped). Every ancestor of the leaf is a candidate —
/// the leaf may be a module file, a module directory, or an item name
/// living inside the parent module.
/// Operation: loop building candidate list, no own calls.
fn candidate_paths(inner: &[String]) -> Vec<String> {
    let mut candidates = Vec::new();
    for len in (1..=inner.len()).rev() {
        let head = &inner[..len];
        let joined = head.join("/");
        candidates.push(format!("src/{joined}.rs"));
        candidates.push(format!("src/{joined}/mod.rs"));
    }
    candidates
}
