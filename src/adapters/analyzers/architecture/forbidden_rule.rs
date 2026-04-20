//! Forbidden Rule — paired glob prohibition on cross-module imports.
//!
//! Each rule has a `from` file-path glob and a `to` file-path glob. A file
//! whose path matches `from` must not import anything that resolves to a
//! file-path matching `to`, unless that candidate also matches one of the
//! `except` globs.
//!
//! Imports are resolved by synthesising candidate file paths from the
//! `crate::<seg1>::<seg2>::…::<segN>` prefix: at every prefix length we
//! consider both the leaf-as-file (`src/<seg1>/…/<segN>.rs`) and the
//! leaf-as-dir (`src/<seg1>/…/<segN>/mod.rs`) layouts. Imports starting
//! with `self`, `super`, `std`, `core`, `alloc`, or an external crate
//! name are skipped — their target has no crate-relative file path, and
//! other architecture rules cover external crates.

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
    let inner = crate_inner_segments(segments)?;
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

/// Strip the `crate::` prefix; return None for imports without a crate-relative
/// target path (`self`, `super`, `std`, `core`, `alloc`, or external crates).
/// Operation: first-segment routing logic.
fn crate_inner_segments(segments: &[String]) -> Option<Vec<String>> {
    let first = segments.first()?;
    if first == "crate" {
        return Some(segments[1..].to_vec());
    }
    None
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
