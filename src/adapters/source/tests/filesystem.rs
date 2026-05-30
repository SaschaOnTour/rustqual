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

// ── Multi-line qual:allow block tests ───────────────────────────

fn parsed_single(path: &str, source: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(source).expect("parse");
    vec![(path.to_string(), source.to_string(), syntax)]
}

#[test]
fn qual_allow_honors_marker_inside_contiguous_comment_block() {
    // Layout:
    //   line 1: // qual:allow(srp) — rustqual false-positive LCOM4=2
    //   line 2: // The struct's methods form one coherent data layer.
    //   line 3: // See docs/rustqual-bugs.md.
    //   line 4: #[derive(Default)]
    //   line 5: pub struct Foo { ... }
    //
    // Without the block-end shift, ANNOTATION_WINDOW=3 from line 1
    // reaches only line 4 — too short to cover the struct on line 5.
    // With the shift, the effective marker line is 3 (last // of the
    // contiguous block) and the window 3..=6 covers the struct.
    let source = "// qual:allow(srp) — rustqual false-positive LCOM4=2\n\
                  // The struct's methods form one coherent data layer.\n\
                  // See docs/rustqual-bugs.md.\n\
                  #[derive(Default)]\n\
                  pub struct Foo { x: i32, y: i32 }\n";
    let parsed = parsed_single("test.rs", source);
    let map = collect_suppression_lines(&parsed);
    let sups = map.get("test.rs").expect("file recorded");
    assert_eq!(sups.len(), 1, "exactly one suppression");
    assert_eq!(
        sups[0].line, 3,
        "marker should be shifted to last // line of the contiguous block (line 3), got {}",
        sups[0].line
    );
}

#[test]
fn qual_allow_does_not_reach_across_blank_lines() {
    // Marker on line 1, blank line on line 2 breaks the block. Marker
    // line stays at 1; struct on line 4 is outside the 3-line window
    // from line 1.
    let source = "// qual:allow(srp)\n\
                  \n\
                  #[derive(Default)]\n\
                  pub struct Foo { x: i32 }\n";
    let parsed = parsed_single("test.rs", source);
    let map = collect_suppression_lines(&parsed);
    let sups = map.get("test.rs").expect("file recorded");
    assert_eq!(sups.len(), 1, "marker still parsed");
    assert_eq!(
        sups[0].line, 1,
        "blank line breaks the block; marker stays at its original line"
    );
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
