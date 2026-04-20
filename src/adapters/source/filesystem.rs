use std::collections::HashSet;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

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

/// Get Rust files changed vs a git ref.
/// Operation: shells out to git and parses output.
pub(crate) fn get_git_changed_files(path: &Path, git_ref: &str) -> Result<Vec<PathBuf>, String> {
    let dir = if path.is_file() {
        path.parent().unwrap_or(path)
    } else {
        path
    };

    let root_output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;
    if !root_output.status.success() {
        return Err("Not a git repository".into());
    }

    let git_root = PathBuf::from(String::from_utf8_lossy(&root_output.stdout).trim());

    let output = std::process::Command::new("git")
        .args([
            "diff",
            "--name-only",
            "--diff-filter=ACMR",
            git_ref,
            "--",
            "*.rs",
        ])
        .current_dir(&git_root)
        .output()
        .map_err(|e| format!("Failed to run git diff: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git diff failed: {}", stderr.trim()));
    }

    let files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| git_root.join(l))
        .collect();

    Ok(files)
}

/// Filter file list to only those present in the changed set.
/// Operation: set-intersection logic using canonical paths.
pub(crate) fn filter_to_changed(all: Vec<PathBuf>, changed: &[PathBuf]) -> Vec<PathBuf> {
    let changed_canonical: HashSet<PathBuf> = changed
        .iter()
        .filter_map(|c| std::fs::canonicalize(c).ok())
        .collect();

    all.into_iter()
        .filter(|f| {
            std::fs::canonicalize(f)
                .map(|c| changed_canonical.contains(&c))
                .unwrap_or(false)
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
    let mut raw = collect_per_file(parsed, |line_num, trimmed| {
        parse_suppression(line_num, trimmed)
    });
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
fn collect_marker_lines<F>(
    parsed: &[(String, String, syn::File)],
    is_marker: F,
) -> std::collections::HashMap<String, std::collections::HashSet<usize>>
where
    F: Fn(&str) -> bool,
{
    parsed
        .iter()
        .filter_map(|(path, source, _)| {
            let ends = compute_comment_block_ends(source);
            let shift = |n: usize| ends.get(&n).copied().unwrap_or(n);
            let lines: std::collections::HashSet<usize> = source
                .lines()
                .enumerate()
                .filter_map(|(i, line)| is_marker(line.trim()).then_some(shift(i + 1)))
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
