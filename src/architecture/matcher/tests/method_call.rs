use crate::architecture::matcher::find_method_call_matches;
use crate::architecture::ViolationKind;

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("fixture must parse")
}

fn find(src: &str, names: &[&str]) -> Vec<crate::architecture::MatchLocation> {
    let ast = parse(src);
    let owned: Vec<String> = names.iter().map(|s| (*s).to_string()).collect();
    find_method_call_matches("fixture.rs", &ast, &owned)
}

// ── Direct method calls (x.method()) ──────────────────────────────────

#[test]
fn matches_direct_method_call_unwrap() {
    let src = r#"
        fn run() {
            let x: Option<i32> = Some(1);
            x.unwrap();
        }
    "#;
    let hits = find(src, &["unwrap"]);
    assert_eq!(hits.len(), 1, "expected one hit: {hits:?}");
    match &hits[0].kind {
        ViolationKind::MethodCall { name, .. } => assert_eq!(name, "unwrap"),
        other => panic!("unexpected kind: {other:?}"),
    }
}

#[test]
fn matches_direct_method_call_expect_with_arg() {
    let src = r#"
        fn run() {
            let x: Option<i32> = Some(1);
            x.expect("should be some");
        }
    "#;
    let hits = find(src, &["expect"]);
    assert_eq!(hits.len(), 1);
}

#[test]
fn matches_chained_method_call() {
    let src = r#"
        fn run() {
            let s = "42";
            s.parse::<i32>().unwrap();
        }
    "#;
    let hits = find(src, &["unwrap"]);
    assert_eq!(hits.len(), 1);
}

#[test]
fn matches_multiple_occurrences_separately() {
    let src = r#"
        fn run() {
            let a: Option<i32> = Some(1);
            let b: Option<i32> = Some(2);
            a.unwrap();
            b.unwrap();
        }
    "#;
    let hits = find(src, &["unwrap"]);
    assert_eq!(hits.len(), 2);
}

// ── UFCS-form calls (Type::method(receiver)) ──────────────────────────

#[test]
fn matches_ufcs_call_option_unwrap() {
    let src = r#"
        fn run() {
            let x: Option<i32> = Some(1);
            Option::unwrap(x);
        }
    "#;
    let hits = find(src, &["unwrap"]);
    assert_eq!(hits.len(), 1, "UFCS form must be caught: {hits:?}");
    match &hits[0].kind {
        ViolationKind::MethodCall { name, .. } => assert_eq!(name, "unwrap"),
        other => panic!("unexpected kind: {other:?}"),
    }
}

#[test]
fn matches_ufcs_call_result_expect() {
    let src = r#"
        fn run() {
            let x: Result<i32, &str> = Ok(1);
            Result::expect(x, "should be ok");
        }
    "#;
    let hits = find(src, &["expect"]);
    assert_eq!(hits.len(), 1);
}

// ── Multiple names in list ────────────────────────────────────────────

#[test]
fn matches_multiple_names_independently() {
    let src = r#"
        fn run() {
            let a: Option<i32> = Some(1);
            let b: Option<i32> = Some(2);
            a.unwrap();
            b.expect("must be some");
        }
    "#;
    let hits = find(src, &["unwrap", "expect"]);
    assert_eq!(hits.len(), 2);
    assert!(hits
        .iter()
        .any(|h| matches!(&h.kind, ViolationKind::MethodCall { name, .. } if name == "unwrap")));
    assert!(hits
        .iter()
        .any(|h| matches!(&h.kind, ViolationKind::MethodCall { name, .. } if name == "expect")));
}

// ── Does NOT match ────────────────────────────────────────────────────

#[test]
fn does_not_match_similar_but_different_name() {
    let src = r#"
        fn run() {
            let x: Option<i32> = Some(1);
            x.unwrap_or(0);
            x.unwrap_or_default();
        }
    "#;
    let hits = find(src, &["unwrap"]);
    assert!(
        hits.is_empty(),
        "unwrap_or and unwrap_or_default must not match 'unwrap': {hits:?}"
    );
}

#[test]
fn does_not_match_name_in_string_or_comment() {
    let src = r#"
        fn run() {
            let s = "unwrap";
            // x.unwrap() mentioned in a comment
            let _ = s;
        }
    "#;
    let hits = find(src, &["unwrap"]);
    assert!(
        hits.is_empty(),
        "strings and comments must not match: {hits:?}"
    );
}

#[test]
fn does_not_match_free_function_with_dissimilar_name() {
    let src = r#"
        fn run() {
            my_helper(1);
        }
        fn my_helper(_x: i32) {}
    "#;
    let hits = find(src, &["unwrap"]);
    assert!(hits.is_empty());
}

#[test]
fn does_not_match_empty_file() {
    let hits = find("", &["unwrap"]);
    assert!(hits.is_empty());
}

// ── Edge cases ────────────────────────────────────────────────────────

#[test]
fn matches_method_call_inside_closure() {
    let src = r#"
        fn run() {
            let xs: Vec<Option<i32>> = vec![Some(1)];
            let _: Vec<i32> = xs.iter().map(|x| x.unwrap()).collect();
        }
    "#;
    let hits = find(src, &["unwrap"]);
    assert_eq!(
        hits.len(),
        1,
        "method call inside closure must match: {hits:?}"
    );
}

#[test]
fn matches_method_call_inside_macro_arg() {
    let src = r#"
        fn run() {
            let x: Option<i32> = Some(1);
            let _ = format!("{}", x.unwrap());
        }
    "#;
    let hits = find(src, &["unwrap"]);
    assert_eq!(
        hits.len(),
        1,
        "method inside macro arg must match: {hits:?}"
    );
}
