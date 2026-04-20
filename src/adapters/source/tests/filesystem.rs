use crate::adapters::source::filesystem::*;
use std::path::PathBuf;

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

#[test]
fn test_collect_rust_files_dot_prefix_path() {
    // Simulates `./src/` — the "." component should not be filtered as hidden
    let dir = tempfile::Builder::new()
        .prefix("rustqual_test_")
        .tempdir()
        .unwrap();
    let sub = dir.path().join("src");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("main.rs"), "fn main() {}").unwrap();

    // Access via ./src by using the parent with a "." prefix
    let dot_path = dir.path().join(".");
    let dot_src = dot_path.join("src");
    let files = collect_rust_files(&dot_src);
    assert!(
        !files.is_empty(),
        "collect_rust_files should find files via ./src path"
    );
}

#[test]
fn test_collect_rust_files_hidden_dir_excluded() {
    let dir = tempfile::Builder::new()
        .prefix("rustqual_test_")
        .tempdir()
        .unwrap();
    let hidden = dir.path().join(".hidden");
    std::fs::create_dir_all(&hidden).unwrap();
    std::fs::write(hidden.join("lib.rs"), "fn foo() {}").unwrap();
    // Also add a visible file
    std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

    let files = collect_rust_files(dir.path());
    assert!(
        files
            .iter()
            .all(|f| !f.to_string_lossy().contains(".hidden")),
        "Hidden directories should be excluded"
    );
    assert!(!files.is_empty(), "Visible files should still be found");
}

#[test]
fn test_collect_rust_files_target_dir_excluded() {
    let dir = tempfile::Builder::new()
        .prefix("rustqual_test_")
        .tempdir()
        .unwrap();
    let target = dir.path().join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("generated.rs"), "fn gen() {}").unwrap();
    std::fs::write(dir.path().join("lib.rs"), "fn lib() {}").unwrap();

    let files = collect_rust_files(dir.path());
    assert!(
        files
            .iter()
            .all(|f| !f.to_string_lossy().contains("target")),
        "target/ directory should be excluded"
    );
    assert!(!files.is_empty());
}

#[test]
fn test_display_path_uses_forward_slashes() {
    let dir = tempfile::Builder::new()
        .prefix("rustqual_test_")
        .tempdir()
        .unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("mod.rs"), "fn f() {}").unwrap();

    let parsed = read_and_parse_files(&collect_rust_files(dir.path()), dir.path());
    assert!(!parsed.is_empty());
    // Display path should use forward slashes, not backslashes
    assert!(
        !parsed[0].0.contains('\\'),
        "Display path should use forward slashes, got: {}",
        parsed[0].0
    );
}

#[test]
fn test_collect_rust_files_dotdot_path() {
    // Simulates `../other/src` — the ".." component should not be filtered as hidden
    let dir = tempfile::Builder::new()
        .prefix("rustqual_test_")
        .tempdir()
        .unwrap();
    let sub = dir.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("lib.rs"), "fn f() {}").unwrap();

    // Access via parent/../sub
    let dotdot_path = dir.path().join("sub").join("..").join("sub");
    let files = collect_rust_files(&dotdot_path);
    assert!(
        !files.is_empty(),
        "collect_rust_files should find files via ../sub path"
    );
}
