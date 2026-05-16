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
/// `None` for everything else — i.e. imports whose first segment is
/// not `crate` / `self` / `super`. That includes the stdlib
/// (`std::…`, `core::…`, `alloc::…`), external crates (`serde::…`,
/// `syn::…`, etc.) and any other unrecognised leading segment, plus
/// resolved paths that still contain a wildcard `*` segment (e.g.
/// `use crate::foo::*;`) — none of those can be turned into concrete
/// candidate file paths in this crate.
/// Operation: first-segment routing + path arithmetic, no own calls.
pub(crate) fn resolve_to_crate_absolute(
    importing_file: &str,
    segments: &[String],
) -> Option<Vec<String>> {
    resolve_to_crate_absolute_in(importing_file, &[], segments)
}

/// Variant that resolves `self::` / `super::` relative to the inline
/// `mod_stack` inside `importing_file`. Pass `&[]` to behave as the
/// file-level resolver.
pub(crate) fn resolve_to_crate_absolute_in(
    importing_file: &str,
    mod_stack: &[String],
    segments: &[String],
) -> Option<Vec<String>> {
    let first = segments.first()?;
    let mut base = file_to_module_segments(importing_file);
    base.extend_from_slice(mod_stack);
    let resolved = match first.as_str() {
        "crate" => segments[1..].to_vec(),
        "self" => {
            base.extend_from_slice(&segments[1..]);
            base
        }
        "super" => {
            let mut i = 0;
            while segments.get(i).is_some_and(|s| s == "super") {
                base.pop()?;
                i += 1;
            }
            base.extend_from_slice(&segments[i..]);
            base
        }
        _ => return None,
    };
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
pub(crate) fn file_to_module_segments(path: &str) -> Vec<String> {
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

// qual:api
/// Build a `module-segments → file-path` index over the workspace
/// files, applying Rust's precedence rule when two files map to the
/// same module identity.
///
/// Both `src/foo.rs` and `src/foo/mod.rs` produce `["foo"]` from
/// [`file_to_module_segments`], so a naive `.collect()` lets iteration
/// order pick the winner — non-deterministic, and the stale-leftover
/// case (refactor switched to single-file style but forgot to delete
/// the old `mod.rs`) silently shadows the live file. Modern Rust
/// rejects the pair as a duplicate-module error; rustqual mirrors the
/// modern convention by deterministically preferring the non-`mod.rs`
/// form. Workspace-resolution callers (`collect_workspace_module_paths`,
/// `collect_file_root_visibility`, …) MUST go through this helper so
/// the precedence is defined in one place.
///
/// Empty-segs (crate roots `src/lib.rs` / `src/main.rs`) are
/// deliberately **not** stored: library and binary crate roots are two
/// separate module trees, not alternatives for the same module
/// identity. Callers that need them collect them by name separately.
/// Operation: linear scan with explicit precedence resolution, no own
/// calls.
pub(crate) fn build_module_segs_to_path_map<'a>(
    files: &[(&'a str, &syn::File)],
) -> std::collections::HashMap<Vec<String>, &'a str> {
    let mut out: std::collections::HashMap<Vec<String>, &'a str> =
        std::collections::HashMap::with_capacity(files.len());
    for (path, _) in files {
        let segs = file_to_module_segments(path);
        if segs.is_empty() {
            continue;
        }
        match out.get(&segs) {
            Some(existing) if existing.replace('\\', "/").ends_with("/mod.rs") => {
                out.insert(segs, *path);
            }
            Some(_) => {
                // Existing entry is the modern `foo.rs` form — keep
                // it; the incoming `foo/mod.rs` is the legacy form.
            }
            None => {
                out.insert(segs, *path);
            }
        }
    }
    out
}

// qual:api
/// True when `path` is the winner (or unique candidate) for its module
/// identity in `segs_to_path` — i.e. it's the file that
/// [`build_module_segs_to_path_map`] picked, or there was never a
/// collision. Files whose `segs` weren't stored (crate roots) also
/// pass.
///
/// Use this gate at every workspace-walker entry point that iterates
/// the raw `files` slice. Without it, stale `foo/mod.rs` leftovers
/// re-introduce their submodule declarations / `pub fn` visibility
/// even after the tie-break helper picked `foo.rs`. Centralised here
/// so the "what does the loser look like" semantic lives next to the
/// builder that defines it.
/// Operation: HashMap lookup + path comparison, no own calls.
pub(crate) fn is_tie_break_winner(
    path: &str,
    segs: &[String],
    segs_to_path: &std::collections::HashMap<Vec<String>, &str>,
) -> bool {
    match segs_to_path.get(segs) {
        Some(winner) => *winner == path,
        // No collision recorded: either the segs were unique, or the
        // builder deliberately skipped them (crate roots). Either way
        // there's no contender to lose to.
        None => true,
    }
}

/// Synthesise the candidate `src/…` file paths for a segment prefix (the
/// `crate::` already stripped). Every ancestor of the leaf is a
/// candidate — the leaf may be a module file, a module directory, or
/// an item name living inside the parent module. For single-segment
/// imports (`use crate::Foo`) we additionally consider `src/lib.rs`
/// and `src/main.rs`, since `Foo` may be declared or re-exported at
/// the crate root. For deeper paths (`use crate::foo::Bar`) the leaf
/// item lives inside `foo`'s own file, so crate-root candidates are
/// not added.
/// Operation: loop building candidate list, no own calls.
fn candidate_paths(inner: &[String]) -> Vec<String> {
    let mut candidates = Vec::new();
    for len in (1..=inner.len()).rev() {
        let head = &inner[..len];
        let joined = head.join("/");
        candidates.push(format!("src/{joined}.rs"));
        candidates.push(format!("src/{joined}/mod.rs"));
    }
    if inner.len() == 1 {
        candidates.push("src/lib.rs".to_string());
        candidates.push("src/main.rs".to_string());
    }
    candidates
}
