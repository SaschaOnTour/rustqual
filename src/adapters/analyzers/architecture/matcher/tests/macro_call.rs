use crate::adapters::analyzers::architecture::matcher::find_macro_calls;
use crate::adapters::analyzers::architecture::ViolationKind;

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("fixture must parse")
}

fn find(src: &str, names: &[&str]) -> Vec<crate::adapters::analyzers::architecture::MatchLocation> {
    let ast = parse(src);
    let owned: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
    find_macro_calls("fixture.rs", &ast, &owned)
}

// ── Direct macro invocations ──────────────────────────────────────────

#[test]
fn matches_println_expression_macro() {
    let src = r#"
        fn run() {
            println!("hello");
        }
    "#;
    let hits = find(src, &["println"]);
    assert_eq!(hits.len(), 1, "expected one hit: {hits:?}");
    match &hits[0].kind {
        ViolationKind::MacroCall { name } => assert_eq!(name, "println"),
        other => panic!("unexpected kind: {other:?}"),
    }
}

#[test]
fn matches_panic_macro() {
    let src = r#"
        fn run() {
            panic!("bad");
        }
    "#;
    let hits = find(src, &["panic"]);
    assert_eq!(hits.len(), 1);
}

#[test]
fn matches_multiple_macros_separately() {
    let src = r#"
        fn run() {
            println!("a");
            eprintln!("b");
            dbg!(42);
        }
    "#;
    let hits = find(src, &["println", "eprintln", "dbg"]);
    assert_eq!(hits.len(), 3);
}

// ── Macros with qualified paths ───────────────────────────────────────

#[test]
fn matches_std_println_via_final_segment() {
    let src = r#"
        fn run() {
            std::println!("hi");
        }
    "#;
    let hits = find(src, &["println"]);
    assert_eq!(
        hits.len(),
        1,
        "qualified std::println! must match by final segment: {hits:?}"
    );
}

// ── Nested macros ─────────────────────────────────────────────────────

#[test]
fn matches_macro_inside_macro_args() {
    let src = r#"
        fn run() {
            let v = vec![format!("inner")];
            let _ = v;
        }
    "#;
    let hits = find(src, &["format"]);
    assert_eq!(
        hits.len(),
        1,
        "macro inside macro arg must be found: {hits:?}"
    );
}

// ── Negative matches ──────────────────────────────────────────────────

#[test]
fn does_not_match_similar_named_macro() {
    let src = r#"
        fn run() {
            println_verbose!("a");
        }
        macro_rules! println_verbose {
            ($($t:tt)*) => {};
        }
    "#;
    let hits = find(src, &["println"]);
    assert!(
        hits.is_empty(),
        "println_verbose must not match 'println': {hits:?}"
    );
}

#[test]
fn does_not_match_macro_name_in_string() {
    let src = r#"
        fn run() {
            let _ = "println!(x)";
        }
    "#;
    let hits = find(src, &["println"]);
    assert!(hits.is_empty());
}

#[test]
fn does_not_match_plain_function_with_macro_name() {
    let src = r#"
        fn run() {
            println();
        }
        fn println() {}
    "#;
    let hits = find(src, &["println"]);
    assert!(
        hits.is_empty(),
        "plain function call must not match macro rule: {hits:?}"
    );
}

#[test]
fn does_not_match_empty_file() {
    let hits = find("", &["panic"]);
    assert!(hits.is_empty());
}

// ── Item-level macros (e.g. `macro_name!(...);` at module level) ──────

#[test]
fn matches_item_level_macro() {
    let src = r#"
        include_str!("readme.md");
    "#;
    let hits = find(src, &["include_str"]);
    assert_eq!(hits.len(), 1, "item-level macro must match: {hits:?}");
}

// ── Line number ───────────────────────────────────────────────────────

#[test]
fn line_number_points_to_bang() {
    let src = "\n\n\nfn run() { println!(\"x\"); }\n";
    let hits = find(src, &["println"]);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 4, "match line should be the call line");
}
