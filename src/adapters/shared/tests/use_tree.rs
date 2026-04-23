//! Tests for `gather_alias_map` — per-file mapping of
//! import-introduced identifiers to their canonical path segments.

use crate::adapters::shared::use_tree::gather_alias_map;

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse")
}

#[test]
fn test_alias_map_simple_use() {
    let f = parse("use foo::bar;");
    let map = gather_alias_map(&f);
    assert_eq!(
        map.get("bar"),
        Some(&vec!["foo".to_string(), "bar".to_string()])
    );
}

#[test]
fn test_alias_map_rename() {
    let f = parse("use foo::bar as baz;");
    let map = gather_alias_map(&f);
    assert_eq!(
        map.get("baz"),
        Some(&vec!["foo".to_string(), "bar".to_string()])
    );
    assert!(
        !map.contains_key("bar"),
        "renamed origin must not leak into the alias map"
    );
}

#[test]
fn test_alias_map_nested_group() {
    let f = parse("use foo::{bar, baz};");
    let map = gather_alias_map(&f);
    assert_eq!(
        map.get("bar"),
        Some(&vec!["foo".to_string(), "bar".to_string()])
    );
    assert_eq!(
        map.get("baz"),
        Some(&vec!["foo".to_string(), "baz".to_string()])
    );
}

#[test]
fn test_alias_map_glob_skipped() {
    let f = parse("use foo::*;");
    let map = gather_alias_map(&f);
    assert!(map.is_empty(), "glob imports must not yield entries");
}

#[test]
fn test_alias_map_self_in_group() {
    let f = parse("use foo::{self, bar};");
    let map = gather_alias_map(&f);
    assert_eq!(map.get("foo"), Some(&vec!["foo".to_string()]));
    assert_eq!(
        map.get("bar"),
        Some(&vec!["foo".to_string(), "bar".to_string()])
    );
}

#[test]
fn test_alias_map_self_renamed_in_group() {
    // `use foo::{self as bar};` parses as Rename { ident: "self",
    // rename: "bar" }. The canonical path must be `[foo]`, not
    // `[foo, self]`, otherwise downstream alias resolution produces
    // a bogus `foo::self::…` target.
    let f = parse("use foo::{self as bar};");
    let map = gather_alias_map(&f);
    assert_eq!(
        map.get("bar"),
        Some(&vec!["foo".to_string()]),
        "self-rename must canonicalise to the parent prefix"
    );
    assert!(
        !map.contains_key("foo"),
        "the renamed binding must not leak the original name"
    );
}

#[test]
fn test_alias_map_crate_prefix() {
    let f = parse("use crate::app::RlmSession;");
    let map = gather_alias_map(&f);
    assert_eq!(
        map.get("RlmSession"),
        Some(&vec![
            "crate".to_string(),
            "app".to_string(),
            "RlmSession".to_string()
        ])
    );
}

#[test]
fn test_alias_map_multiple_top_level_uses() {
    let f = parse(
        "use foo::A;\n\
         use bar::B;",
    );
    let map = gather_alias_map(&f);
    assert_eq!(
        map.get("A"),
        Some(&vec!["foo".to_string(), "A".to_string()])
    );
    assert_eq!(
        map.get("B"),
        Some(&vec!["bar".to_string(), "B".to_string()])
    );
}
