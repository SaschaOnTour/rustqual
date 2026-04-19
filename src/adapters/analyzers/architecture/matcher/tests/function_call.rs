use crate::adapters::analyzers::architecture::matcher::find_function_call_matches;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("test fixture must parse")
}

fn find(src: &str, paths: &[&str]) -> Vec<MatchLocation> {
    let ast = parse(src);
    let owned: Vec<String> = paths.iter().map(|p| (*p).to_string()).collect();
    find_function_call_matches("fixture.rs", &ast, &owned)
}

// ── clean fixtures ───────────────────────────────────────────────────

#[test]
fn clean_file_no_matches() {
    let src = r#"
        fn main() {
            let x = 1 + 1;
            println!("{x}");
        }
    "#;
    assert!(find(src, &["Box::new"]).is_empty());
}

// ── primary behaviour ────────────────────────────────────────────────

#[test]
fn matches_two_segment_path() {
    let src = r#"
        fn main() {
            let _x = Box::new(42);
        }
    "#;
    let hits = find(src, &["Box::new"]);
    assert_eq!(hits.len(), 1, "{hits:?}");
    match &hits[0].kind {
        ViolationKind::FunctionCall { rendered_path } => {
            assert_eq!(rendered_path, "Box::new");
        }
        other => panic!("unexpected kind: {other:?}"),
    }
}

#[test]
fn matches_deep_path() {
    let src = r#"
        fn main() {
            std::process::exit(1);
        }
    "#;
    let hits = find(src, &["std::process::exit"]);
    assert_eq!(hits.len(), 1);
}

#[test]
fn matches_single_segment() {
    let src = r#"
        fn my_func() {}
        fn caller() { my_func(); }
    "#;
    let hits = find(src, &["my_func"]);
    assert_eq!(hits.len(), 1);
}

// ── full-path specificity ────────────────────────────────────────────

#[test]
fn does_not_partial_match_final_segment() {
    // `forbid_function_call = ["Box::new"]` must NOT match a random `new` call.
    let src = r#"
        struct Thing;
        impl Thing { fn new() -> Self { Self } }
        fn f() { Thing::new(); }
    "#;
    let hits = find(src, &["Box::new"]);
    assert!(hits.is_empty(), "{hits:?}");
}

#[test]
fn does_not_match_prefix() {
    // Configured "std::process::exit" should not match "process::exit".
    let src = r#"
        fn f() { process::exit(0); }
    "#;
    let hits = find(src, &["std::process::exit"]);
    assert!(hits.is_empty(), "{hits:?}");
}

// ── independence from method_call ────────────────────────────────────

#[test]
fn ignores_dot_syntax_method_call() {
    let src = r#"
        fn f() { let v: Option<i32> = None; v.unwrap(); }
    "#;
    let hits = find(src, &["unwrap", "Option::unwrap"]);
    assert!(
        hits.is_empty(),
        "method calls are not function_call territory: {hits:?}"
    );
}

#[test]
fn matches_ufcs_when_full_path_configured() {
    // `Option::unwrap(x)` IS a function-call expression; forbid_function_call
    // legitimately matches it when the full path is configured.
    let src = r#"
        fn f() { let v: Option<i32> = None; Option::unwrap(v); }
    "#;
    let hits = find(src, &["Option::unwrap"]);
    assert_eq!(hits.len(), 1);
}

// ── descent into nested calls ────────────────────────────────────────

#[test]
fn descends_into_call_arguments() {
    let src = r#"
        fn outer<T>(x: T) -> T { x }
        fn main() { outer(Box::new(42)); }
    "#;
    let hits = find(src, &["Box::new"]);
    assert_eq!(hits.len(), 1, "{hits:?}");
}

#[test]
fn descends_into_macro_tokens() {
    let src = r#"
        fn main() {
            format!("{}", Box::new(42).as_ref());
        }
    "#;
    let hits = find(src, &["Box::new"]);
    assert_eq!(hits.len(), 1, "{hits:?}");
}

// ── turbofish tolerance ──────────────────────────────────────────────

#[test]
fn strips_turbofish_before_match() {
    let src = r#"
        fn main() {
            Vec::<i32>::new();
        }
    "#;
    let hits = find(src, &["Vec::new"]);
    assert_eq!(hits.len(), 1, "turbofish must not block match: {hits:?}");
}

// ── configured list semantics ────────────────────────────────────────

#[test]
fn multiple_configured_paths_each_checked() {
    let src = r#"
        fn main() {
            Box::new(1);
            std::process::exit(1);
        }
    "#;
    let hits = find(src, &["Box::new", "std::process::exit"]);
    assert_eq!(hits.len(), 2);
}
