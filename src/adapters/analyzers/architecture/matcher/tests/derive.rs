use crate::adapters::analyzers::architecture::matcher::find_derive_matches;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("test fixture must parse")
}

fn find(src: &str, names: &[&str]) -> Vec<MatchLocation> {
    let ast = parse(src);
    let owned: Vec<String> = names.iter().map(|n| (*n).to_string()).collect();
    find_derive_matches("fixture.rs", &ast, &owned)
}

fn matched_traits(hits: &[MatchLocation]) -> Vec<String> {
    hits.iter()
        .filter_map(|h| match &h.kind {
            ViolationKind::Derive { trait_name, .. } => Some(trait_name.clone()),
            _ => None,
        })
        .collect()
}

fn matched_items(hits: &[MatchLocation]) -> Vec<String> {
    hits.iter()
        .filter_map(|h| match &h.kind {
            ViolationKind::Derive { item_name, .. } => Some(item_name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn clean_file_no_matches() {
    let src = "pub struct Foo; pub enum Bar { A, B }";
    assert!(find(src, &["Serialize"]).is_empty());
}

#[test]
fn matches_single_derive_on_struct() {
    let src = r#"
        #[derive(Serialize)]
        pub struct Foo;
    "#;
    let hits = find(src, &["Serialize"]);
    assert_eq!(hits.len(), 1);
    assert_eq!(matched_traits(&hits), vec!["Serialize"]);
    assert_eq!(matched_items(&hits), vec!["Foo"]);
}

#[test]
fn matches_when_in_list_of_derives() {
    let src = r#"
        #[derive(Debug, Serialize, Clone)]
        pub struct Foo;
    "#;
    let hits = find(src, &["Serialize"]);
    assert_eq!(hits.len(), 1);
    assert_eq!(matched_traits(&hits), vec!["Serialize"]);
}

#[test]
fn matches_multiple_banned_in_same_derive() {
    let src = r#"
        #[derive(Serialize, Deserialize)]
        pub struct Foo;
    "#;
    let hits = find(src, &["Serialize", "Deserialize"]);
    assert_eq!(hits.len(), 2);
}

#[test]
fn matches_on_enum() {
    let src = r#"
        #[derive(Serialize)]
        pub enum Color { Red, Green }
    "#;
    let hits = find(src, &["Serialize"]);
    assert_eq!(hits.len(), 1);
    assert_eq!(matched_items(&hits), vec!["Color"]);
}

#[test]
fn matches_on_union() {
    let src = r#"
        #[derive(Copy, Clone)]
        pub union Both { a: u32, b: f32 }
    "#;
    let hits = find(src, &["Copy"]);
    assert_eq!(hits.len(), 1);
    assert_eq!(matched_items(&hits), vec!["Both"]);
}

#[test]
fn ignores_unlisted_derives() {
    let src = r#"
        #[derive(Debug, Clone)]
        pub struct Foo;
    "#;
    assert!(find(src, &["Serialize"]).is_empty());
}

#[test]
fn matches_through_fully_qualified_path() {
    // `#[derive(serde::Serialize)]` should match when configured as
    // `"Serialize"` (final segment comparison).
    let src = r#"
        #[derive(serde::Serialize)]
        pub struct Foo;
    "#;
    let hits = find(src, &["Serialize"]);
    assert_eq!(hits.len(), 1);
}

#[test]
fn matches_nested_in_impl() {
    // Derives on items inside impl blocks / nested modules should also match.
    let src = r#"
        pub mod inner {
            #[derive(Deserialize)]
            pub struct Nested;
        }
    "#;
    let hits = find(src, &["Deserialize"]);
    assert_eq!(hits.len(), 1);
}

#[test]
fn non_derive_attrs_ignored() {
    let src = r#"
        #[allow(dead_code)]
        #[inline]
        pub struct Foo;
    "#;
    assert!(find(src, &["Serialize", "allow", "inline"]).is_empty());
}

#[test]
fn multiple_derive_attributes_each_checked() {
    // A struct may have multiple `#[derive(...)]` attributes.
    let src = r#"
        #[derive(Debug)]
        #[derive(Serialize)]
        pub struct Foo;
    "#;
    let hits = find(src, &["Serialize"]);
    assert_eq!(hits.len(), 1);
}
