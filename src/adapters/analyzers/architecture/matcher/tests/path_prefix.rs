use crate::adapters::analyzers::architecture::matcher::find_path_prefix_matches;
use crate::adapters::analyzers::architecture::ViolationKind;

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("test fixture must parse")
}

fn find(
    src: &str,
    prefixes: &[&str],
) -> Vec<crate::adapters::analyzers::architecture::MatchLocation> {
    let ast = parse(src);
    let owned: Vec<String> = prefixes.iter().map(|s| (*s).to_string()).collect();
    find_path_prefix_matches("fixture.rs", &ast, &owned)
}

// ── Clean fixture (no match expected) ─────────────────────────────────

#[test]
fn clean_file_produces_no_matches() {
    let src = r#"
        use std::collections::HashMap;
        fn foo() -> Option<HashMap<String, i32>> { None }
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(
        hits.is_empty(),
        "clean file should produce no hits: {hits:?}"
    );
}

// ── Position 1: use statement ─────────────────────────────────────────

#[test]
fn matches_use_statement() {
    let src = r#"
        use tokio::spawn;
    "#;
    let hits = find(src, &["tokio::"]);
    assert_eq!(hits.len(), 1, "expected exactly one hit: {hits:?}");
    match &hits[0].kind {
        ViolationKind::PathPrefix {
            prefix,
            rendered_path,
        } => {
            assert_eq!(prefix, "tokio::");
            assert!(rendered_path.starts_with("tokio::"));
        }
        other => panic!("unexpected kind: {other:?}"),
    }
}

#[test]
fn matches_nested_use_groups() {
    let src = r#"
        use tokio::{spawn, sync::Mutex};
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(!hits.is_empty(), "nested use must match");
}

// ── Position 2: function call ─────────────────────────────────────────

#[test]
fn matches_function_call_path() {
    let src = r#"
        fn run() {
            tokio::spawn(async {});
        }
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(
        hits.iter().any(|h| matches!(
            &h.kind,
            ViolationKind::PathPrefix { prefix, .. } if prefix == "tokio::"
        )),
        "function call path should match: {hits:?}"
    );
}

// ── Position 3: attribute ─────────────────────────────────────────────

#[test]
fn matches_attribute_path() {
    let src = r#"
        #[tokio::main]
        async fn main() {}
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(
        hits.iter().any(|h| matches!(
            &h.kind,
            ViolationKind::PathPrefix { rendered_path, .. } if rendered_path.starts_with("tokio::")
        )),
        "attribute path should match: {hits:?}"
    );
}

// ── Position 4: impl for trait ────────────────────────────────────────

#[test]
fn matches_impl_trait_for_type() {
    let src = r#"
        struct X;
        impl tokio::io::AsyncRead for X {}
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(!hits.is_empty(), "impl trait path should match: {hits:?}");
}

// ── Position 5: return type ───────────────────────────────────────────

#[test]
fn matches_return_type() {
    let src = r#"
        fn foo() -> tokio::io::Result<()> { unimplemented!() }
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(!hits.is_empty(), "return type should match: {hits:?}");
}

// ── Position 6: type reference in let/field ───────────────────────────

#[test]
fn matches_let_type_annotation() {
    let src = r#"
        fn foo() {
            let _x: tokio::sync::Mutex<i32>;
        }
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(
        !hits.is_empty(),
        "let type annotation should match: {hits:?}"
    );
}

#[test]
fn matches_struct_field_type() {
    let src = r#"
        struct Holder {
            lock: tokio::sync::Mutex<i32>,
        }
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(!hits.is_empty(), "struct field type should match: {hits:?}");
}

// ── Position 7: generic bound ─────────────────────────────────────────

#[test]
fn matches_generic_bound() {
    let src = r#"
        fn generic<T: tokio::io::AsyncRead>(_t: T) {}
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(!hits.is_empty(), "generic bound should match: {hits:?}");
}

#[test]
fn matches_where_clause_bound() {
    let src = r#"
        fn generic<T>(_t: T) where T: tokio::io::AsyncRead {}
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(
        !hits.is_empty(),
        "where-clause bound should match: {hits:?}"
    );
}

// ── Position 8: extern crate ──────────────────────────────────────────

#[test]
fn matches_extern_crate() {
    let src = r#"
        extern crate tokio;
    "#;
    let hits = find(src, &["tokio"]);
    assert!(!hits.is_empty(), "extern crate should match: {hits:?}");
}

// ── Multiple prefixes, multiple hits ──────────────────────────────────

#[test]
fn reports_each_occurrence_separately() {
    let src = r#"
        use tokio::spawn;
        fn run() {
            tokio::spawn(async {});
            tokio::spawn(async {});
        }
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(
        hits.len() >= 3,
        "expected ≥3 hits (one use + two calls), got {}: {hits:?}",
        hits.len()
    );
}

#[test]
fn multiple_prefixes_both_match() {
    let src = r#"
        use tokio::spawn;
        use anyhow::Result;
    "#;
    let hits = find(src, &["tokio::", "anyhow::"]);
    let has_tokio = hits.iter().any(|h| {
        matches!(
            &h.kind,
            ViolationKind::PathPrefix { prefix, .. } if prefix == "tokio::"
        )
    });
    let has_anyhow = hits.iter().any(|h| {
        matches!(
            &h.kind,
            ViolationKind::PathPrefix { prefix, .. } if prefix == "anyhow::"
        )
    });
    assert!(
        has_tokio && has_anyhow,
        "both prefixes should produce hits: {hits:?}"
    );
}

// ── Does NOT match things that aren't paths ───────────────────────────

#[test]
fn does_not_match_string_containing_prefix() {
    // Strings and comments are not AST paths — they must not match.
    let src = r#"
        fn foo() {
            let s = "tokio::spawn is great";
            // tokio::spawn mentioned in comment
            let _ = s;
        }
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(
        hits.is_empty(),
        "string and comment mentions must not match: {hits:?}"
    );
}

#[test]
fn does_not_match_similar_but_different_prefix() {
    let src = r#"
        use tokio_util::codec::Decoder;
    "#;
    let hits = find(src, &["tokio::"]);
    assert!(
        hits.is_empty(),
        "tokio_util is not tokio:: and must not match: {hits:?}"
    );
}

// ── Bare `use tokio;` must match `"tokio::"` prefix ───────────────────

#[test]
fn bare_crate_use_matches_trailing_colon_prefix() {
    let src = r#"
        use tokio;
    "#;
    let hits = find(src, &["tokio::"]);
    assert_eq!(
        hits.len(),
        1,
        "`use tokio;` must match the `tokio::` prefix: {hits:?}"
    );
}

#[test]
fn bare_crate_rename_matches_trailing_colon_prefix() {
    let src = r#"
        use tokio as t;
    "#;
    let hits = find(src, &["tokio::"]);
    assert_eq!(
        hits.len(),
        1,
        "`use tokio as t;` must match the `tokio::` prefix: {hits:?}"
    );
}

#[test]
fn bare_prefix_matches_exact_and_segment_boundary_only() {
    // Prefix without trailing `::` matches exact crate and segment-
    // boundary paths, but NOT partial name overlap (tokio_util vs tokio).
    let src = r#"
        use tokio_util::codec::Decoder;
    "#;
    let hits = find(src, &["tokio"]);
    assert!(
        hits.is_empty(),
        "bare prefix `tokio` must not match `tokio_util`: {hits:?}"
    );
}
