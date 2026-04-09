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
                    s == "target" || s.starts_with('.')
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

/// Collect all suppression comment lines from source files.
/// Trivial: delegates to collect_per_file with parse_suppression.
pub(crate) fn collect_suppression_lines(
    parsed: &[(String, String, syn::File)],
) -> std::collections::HashMap<String, Vec<Suppression>> {
    collect_per_file(parsed, |line_num, trimmed| {
        parse_suppression(line_num, trimmed)
    })
}

/// Collect `// qual:api` marker line numbers per file.
/// Trivial: delegates to collect_per_file with is_api_marker.
pub(crate) fn collect_api_lines(
    parsed: &[(String, String, syn::File)],
) -> std::collections::HashMap<String, std::collections::HashSet<usize>> {
    collect_per_file(parsed, |line_num, trimmed| {
        crate::findings::is_api_marker(trimmed).then_some(line_num)
    })
    .into_iter()
    .map(|(k, v)| (k, v.into_iter().collect()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_to_changed_matching() {
        let dir = tempfile::Builder::new()
            .prefix("rustqual_test_")
            .tempdir()
            .unwrap();
        let a = dir.path().join("a.rs");
        let b = dir.path().join("b.rs");
        let c = dir.path().join("c.rs");
        std::fs::write(&a, "").unwrap();
        std::fs::write(&b, "").unwrap();
        std::fs::write(&c, "").unwrap();

        let all = vec![a.clone(), b, c.clone()];
        let changed = vec![a, c];
        let result = filter_to_changed(all, &changed);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_to_changed_none_matching() {
        let dir = tempfile::Builder::new()
            .prefix("rustqual_test_")
            .tempdir()
            .unwrap();
        let a = dir.path().join("a.rs");
        let d = dir.path().join("d.rs");
        std::fs::write(&a, "").unwrap();
        std::fs::write(&d, "").unwrap();

        let all = vec![a];
        let changed = vec![d];
        let result = filter_to_changed(all, &changed);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_to_changed_empty_changed() {
        let dir = tempfile::Builder::new()
            .prefix("rustqual_test_")
            .tempdir()
            .unwrap();
        let a = dir.path().join("a.rs");
        std::fs::write(&a, "").unwrap();

        let all = vec![a];
        let changed: Vec<PathBuf> = vec![];
        let result = filter_to_changed(all, &changed);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_to_changed_empty_all() {
        let all: Vec<PathBuf> = vec![];
        let changed: Vec<PathBuf> = vec![PathBuf::from("/tmp/x.rs")];
        let result = filter_to_changed(all, &changed);
        assert!(result.is_empty());
    }

    #[test]
    fn test_get_git_changed_files_not_git_repo() {
        let dir = tempfile::Builder::new()
            .prefix("rustqual_test_")
            .tempdir()
            .unwrap();
        let result = get_git_changed_files(dir.path(), "HEAD");
        assert!(result.is_err());
    }
}
