use crate::adapters::source::filesystem::*;

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

/// In a fresh tempdir, put a `.rs` file inside `excluded_subdir` and a
/// visible `.rs` at the root, then collect — returning the found paths.
fn collect_with_excluded_subdir(excluded_subdir: &str) -> Vec<std::path::PathBuf> {
    let dir = tempfile::Builder::new()
        .prefix("rustqual_test_")
        .tempdir()
        .unwrap();
    let sub = dir.path().join(excluded_subdir);
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("excluded.rs"), "fn x() {}").unwrap();
    std::fs::write(dir.path().join("visible.rs"), "fn v() {}").unwrap();
    collect_rust_files(dir.path())
}

#[test]
fn collect_rust_files_excludes_hidden_and_target_dirs() {
    // Hidden (`.`-prefixed) and `target/` directories are skipped, but a
    // visible sibling file is still found. (label, excluded_subdir)
    for (label, excluded) in [("hidden dir", ".hidden"), ("target dir", "target")] {
        let files = collect_with_excluded_subdir(excluded);
        assert!(
            files
                .iter()
                .all(|f| !f.to_string_lossy().contains(excluded)),
            "case {label}: `{excluded}/` should be excluded"
        );
        assert!(
            !files.is_empty(),
            "case {label}: the visible sibling file should still be found"
        );
    }
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

// ── Multi-line qual:allow block tests ───────────────────────────

fn parsed_single(path: &str, source: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(source).expect("parse");
    vec![(path.to_string(), source.to_string(), syntax)]
}

/// Parse `source`, collect suppression markers, and return the (single)
/// suppression's effective line for `test.rs`.
fn single_suppression_line(source: &str) -> usize {
    let parsed = parsed_single("test.rs", source);
    let map = collect_suppression_lines(&parsed);
    let sups = map.get("test.rs").expect("file recorded");
    assert_eq!(sups.len(), 1, "exactly one suppression");
    sups[0].line
}

#[test]
fn qual_allow_marker_line_follows_contiguous_comment_block() {
    // A `qual:allow` marker's effective line shifts to the LAST `//` line of
    // its contiguous comment block (so ANNOTATION_WINDOW=3 reaches the item a
    // multi-line rationale would otherwise push out of range); a blank line
    // breaks the block and the marker stays on its own line. (label, source,
    // expected_line)
    let cases: &[(&str, &str, usize)] = &[
        (
            "contiguous 3-line block shifts to line 3",
            "// qual:allow(srp, god_struct) reason: \"rustqual false-positive LCOM4=2\"\n\
             // The struct's methods form one coherent data layer.\n\
             // See docs/rustqual-bugs.md.\n\
             #[derive(Default)]\n\
             pub struct Foo { x: i32, y: i32 }\n",
            3,
        ),
        (
            "blank line breaks the block; marker stays at line 1",
            "// qual:allow(srp, god_struct) reason: \"x\"\n\
             \n\
             #[derive(Default)]\n\
             pub struct Foo { x: i32 }\n",
            1,
        ),
    ];
    for (label, source, expected) in cases {
        assert_eq!(single_suppression_line(source), *expected, "case {label}");
    }
}

#[test]
fn qual_api_marker_also_honors_contiguous_block() {
    // `// qual:api` is a separate annotation but uses the same window
    // for matching — it must benefit from the same block-end shift.
    let source = "// qual:api\n\
                  // This is the public API entry point.\n\
                  // Keep this function stable.\n\
                  #[inline]\n\
                  pub fn entry() {}\n";
    let parsed = parsed_single("test.rs", source);
    let lines = collect_api_lines(&parsed);
    let file_lines = lines.get("test.rs").expect("file recorded");
    assert!(
        file_lines.contains(&3),
        "api marker should be shifted to block-end line 3, got {:?}",
        file_lines
    );
}
