use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::adapters::suppression::qual_allow::{detect_invalid_qual_allow, InvalidQualAllow};
use crate::config::Config;
use crate::findings::{parse_suppression, Suppression};

/// Collect Rust source files from a path (file or directory).
/// Operation: file system logic with filtering.
pub(crate) fn collect_rust_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        if path.extension().is_some_and(|ext| ext == "rs") {
            return vec![path.to_path_buf()];
        } else {
            eprintln!("Warning: {} is not a Rust file", path.display());
            return vec![];
        }
    }

    WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().is_some_and(|ext| ext == "rs")
                && !e.path().components().any(|c| {
                    let s = c.as_os_str().to_string_lossy();
                    s == "target" || (s.starts_with('.') && s != "." && s != "..")
                })
        })
        .map(|e| e.into_path())
        .collect()
}

/// Collect and filter Rust files for analysis.
/// Trivial: iterator chain with lenient closures.
pub(crate) fn collect_filtered_files(path: &Path, config: &Config) -> Vec<PathBuf> {
    collect_rust_files(path)
        .into_iter()
        .filter(|f| {
            let rel = f
                .strip_prefix(path)
                .unwrap_or(f)
                .to_string_lossy()
                .replace('\\', "/");
            !config.is_excluded_file(&rel)
        })
        .collect()
}

/// Read and parse all Rust files, returning parsed syntax trees with source.
/// Operation: parallel file reading with error handling logic.
pub(crate) fn read_and_parse_files(
    files: &[PathBuf],
    base_path: &Path,
) -> Vec<(String, String, syn::File)> {
    let file_contents: Vec<(String, String)> = {
        use rayon::prelude::*;
        files
            .par_iter()
            .filter_map(|file_path| {
                let source = match std::fs::read_to_string(file_path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Warning: Could not read {}: {e}", file_path.display());
                        return None;
                    }
                };
                let display_path = file_path
                    .strip_prefix(base_path)
                    .unwrap_or(file_path)
                    .to_string_lossy()
                    .replace('\\', "/");
                Some((display_path, source))
            })
            .collect()
    };

    file_contents
        .into_iter()
        .filter_map(|(path, source)| match syn::parse_file(&source) {
            Ok(syntax) => Some((path, source, syntax)),
            Err(e) => {
                eprintln!("Warning: Could not parse {path}: {e}");
                None
            }
        })
        .collect()
}

/// Scan source lines and collect per-file results via a closure.
/// Trivial: generic iteration infrastructure, no own calls.
fn collect_per_file<T, F>(
    parsed: &[(String, String, syn::File)],
    extract: F,
) -> std::collections::HashMap<String, Vec<T>>
where
    F: Fn(usize, &str) -> Option<T>,
{
    let mut result = std::collections::HashMap::new();
    for (path, source, _) in parsed {
        let items: Vec<T> = source
            .lines()
            .enumerate()
            .filter_map(|(i, line)| extract(i + 1, line.trim()))
            .collect();
        if !items.is_empty() {
            result.insert(path.clone(), items);
        }
    }
    result
}

/// Map 1-based line number → last line of its contiguous `//`-comment
/// block. A block is a run of lines whose `trim_start()` begins with
/// `//`; any other line (code, blank) terminates the block. Used to
/// shift annotation markers to the block's end so multi-line rationales
/// still match items within `ANNOTATION_WINDOW` of the *last* comment
/// line (Bug 3). Lines outside any comment block are absent from the map.
/// Operation: linear scan with run detection.
pub(crate) fn compute_comment_block_ends(source: &str) -> std::collections::HashMap<usize, usize> {
    let mut map = std::collections::HashMap::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("//") {
            let start = i;
            let mut end = i;
            while end + 1 < lines.len() && lines[end + 1].trim_start().starts_with("//") {
                end += 1;
            }
            for j in start..=end {
                map.insert(j + 1, end + 1);
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    map
}

/// Collect all suppression comment lines from source files. The
/// effective line of each suppression is shifted to the end of the
/// contiguous `//`-comment block containing the marker, so multi-line
/// rationales still match items within `ANNOTATION_WINDOW` of the
/// block's last comment (Bug 3).
/// Operation: collects raw markers, then rewrites `.line` per file
/// via the block-ends map.
pub(crate) fn collect_suppression_lines(
    parsed: &[(String, String, syn::File)],
) -> std::collections::HashMap<String, Vec<Suppression>> {
    let mut raw = collect_per_file(parsed, parse_suppression);
    parsed.iter().for_each(|(path, source, _)| {
        if let Some(items) = raw.get_mut(path) {
            let ends = compute_comment_block_ends(source);
            items.iter_mut().for_each(|s| {
                if let Some(&end) = ends.get(&s.line) {
                    s.line = end;
                }
            });
        }
    });
    raw
}

/// Side-channel collector for `// qual:allow(<unknown>)` typo markers.
/// Returns `(line, bad_spec)` pairs per file; line is shifted to the
/// end of its `//`-comment block, mirroring `collect_suppression_lines`.
/// Kept separate from real `Suppression` entries so these markers
/// never enter the suppression-application passes (where empty
/// dimensions would silently suppress every category in the window).
/// Orphan-suppression detection consumes this map directly.
pub(crate) fn collect_invalid_qual_allow_lines(
    parsed: &[(String, String, syn::File)],
) -> std::collections::HashMap<String, Vec<(usize, InvalidQualAllow)>> {
    let mut raw = collect_per_file(parsed, |line_num, trimmed| {
        detect_invalid_qual_allow(trimmed).map(|kind| (line_num, kind))
    });
    parsed.iter().for_each(|(path, source, _)| {
        if let Some(items) = raw.get_mut(path) {
            let ends = compute_comment_block_ends(source);
            items.iter_mut().for_each(|(line, _)| {
                if let Some(&end) = ends.get(line) {
                    *line = end;
                }
            });
        }
    });
    raw
}

/// Collect `// qual:api` marker line numbers per file. Each recorded
/// line is shifted to the end of its contiguous `//`-comment block so
/// multi-line annotations match items within `ANNOTATION_WINDOW` of
/// the block's last comment (Bug 3).
/// Operation: per-file collection with block-end rewrite.
pub(crate) fn collect_api_lines(
    parsed: &[(String, String, syn::File)],
) -> std::collections::HashMap<String, std::collections::HashSet<usize>> {
    collect_marker_lines(parsed, crate::findings::is_api_marker)
}

/// Collect `// qual:test_helper` marker line numbers per file, with
/// the same block-end shift as `collect_api_lines`.
/// Trivial: delegates to `collect_marker_lines`.
pub(crate) fn collect_test_helper_lines(
    parsed: &[(String, String, syn::File)],
) -> std::collections::HashMap<String, std::collections::HashSet<usize>> {
    collect_marker_lines(
        parsed,
        crate::adapters::suppression::qual_allow::is_test_helper_marker,
    )
}

/// Shared implementation for marker-line collectors that produce a
/// `HashSet<usize>` per file. Applies the contiguous `//`-block
/// end-shift so multi-line rationales preceding a marker still match
/// items within `ANNOTATION_WINDOW` of the block's last line.
/// Operation: collect raw marker lines, then map each to its block-end.
/// Lines strictly *inside* a multi-line string literal. Test fixtures embed
/// rustqual's own markers as data (`let code = r#"… // qual:api …"#;`), and a
/// raw line scan would read those as annotations on the enclosing file —
/// phantom markers, and once markers are verified, phantom findings.
///
/// Only the interior counts: the opening and closing lines carry real code,
/// so a marker sharing a line with a literal is still collected.
/// Integration: literal-span collection + interior expansion.
fn literal_spans(syntax: &syn::File) -> LiteralSpans {
    let mut collector = LiteralSpans::default();
    syn::visit::Visit::visit_file(&mut collector, syntax);
    collector
}

/// Line/column extents of every string literal, including those hidden inside
/// macro token streams (`assert_eq!(x, r#"…"#)`).
///
/// Columns matter: the opening and closing lines of a multi-line literal carry
/// *both* string content and real source, so a line-set filter cannot tell
/// `// qual:api example"#;` (literal text) from a genuine trailing marker.
#[derive(Default)]
struct LiteralSpans {
    /// `(start_line, start_col, end_line, end_col)`, columns 0-based.
    spans: Vec<(usize, usize, usize, usize)>,
}

impl LiteralSpans {
    /// True when the text starting at `column` on `line` lies inside a string
    /// literal. Interior lines are wholly inside; on the boundary lines only
    /// the part within the literal's columns counts.
    /// Operation: span containment test, no own calls.
    fn covers(&self, line: usize, column: usize) -> bool {
        self.spans.iter().any(|&(sl, sc, el, ec)| {
            if line < sl || line > el {
                return false;
            }
            let after_start = line > sl || column >= sc;
            let before_end = line < el || column < ec;
            after_start && before_end
        })
    }

    /// Record one literal's extent.
    /// Operation: span projection, no own calls.
    fn push(&mut self, span: proc_macro2::Span) {
        let (start, end) = (span.start(), span.end());
        self.spans
            .push((start.line, start.column, end.line, end.column));
    }
}

impl<'ast> syn::visit::Visit<'ast> for LiteralSpans {
    fn visit_lit_str(&mut self, node: &'ast syn::LitStr) {
        self.push(syn::spanned::Spanned::span(node));
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        node.tokens.clone().into_iter().for_each(|tt| {
            if let proc_macro2::TokenTree::Literal(lit) = tt {
                self.push(lit.span());
            }
        });
        syn::visit::visit_macro(self, node);
    }
}

fn collect_marker_lines<F>(
    parsed: &[(String, String, syn::File)],
    is_marker: F,
) -> std::collections::HashMap<String, std::collections::HashSet<usize>>
where
    F: Fn(&str) -> bool,
{
    parsed
        .iter()
        .filter_map(|(path, source, syntax)| {
            let ends = compute_comment_block_ends(source);
            let shift = |n: usize| ends.get(&n).copied().unwrap_or(n);
            let literals = literal_spans(syntax);
            let lines: std::collections::HashSet<usize> = source
                .lines()
                .enumerate()
                .filter_map(|(i, line)| {
                    let trimmed = line.trim_start();
                    let column = line.len() - trimmed.len();
                    let marker = is_marker(trimmed.trim_end());
                    (marker && !literals.covers(i + 1, column)).then_some(shift(i + 1))
                })
                .collect();
            if lines.is_empty() {
                None
            } else {
                Some((path.clone(), lines))
            }
        })
        .collect()
}

/// Collect `// qual:allow(unsafe)` marker line numbers per file.
/// Trivial: delegates to collect_per_file with is_unsafe_allow_marker.
pub(crate) fn collect_unsafe_allow_lines(
    parsed: &[(String, String, syn::File)],
) -> std::collections::HashMap<String, std::collections::HashSet<usize>> {
    collect_per_file(parsed, |line_num, trimmed| {
        crate::findings::is_unsafe_allow_marker(trimmed).then_some(line_num)
    })
    .into_iter()
    .map(|(k, v)| (k, v.into_iter().collect()))
    .collect()
}

/// Collect `// qual:recursive` marker line numbers per file.
/// Trivial: delegates to collect_per_file with is_recursive_marker.
pub(crate) fn collect_recursive_lines(
    parsed: &[(String, String, syn::File)],
) -> std::collections::HashMap<String, std::collections::HashSet<usize>> {
    collect_per_file(parsed, |line_num, trimmed| {
        crate::findings::is_recursive_marker(trimmed).then_some(line_num)
    })
    .into_iter()
    .map(|(k, v)| (k, v.into_iter().collect()))
    .collect()
}

/// Collect `// qual:inverse(fn_name)` marker lines per file.
/// Trivial: delegates to collect_per_file with parse_inverse_marker.
pub(crate) fn collect_inverse_lines(
    parsed: &[(String, String, syn::File)],
) -> std::collections::HashMap<String, Vec<(usize, String)>> {
    collect_per_file(parsed, |line_num, trimmed| {
        crate::findings::parse_inverse_marker(trimmed).map(|name| (line_num, name))
    })
}
