//! Tests for `gather_alias_map` — per-file mapping of
//! import-introduced identifiers to their canonical path segments
//! plus the leading-colon (absolute-root) bit.

use crate::adapters::shared::use_tree::{gather_alias_map, AliasTarget};

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse")
}

fn relative(segs: &[&str]) -> AliasTarget {
    AliasTarget::relative(segs.iter().map(|s| s.to_string()).collect())
}

fn absolute(segs: &[&str]) -> AliasTarget {
    AliasTarget {
        segments: segs.iter().map(|s| s.to_string()).collect(),
        absolute_root: true,
    }
}

#[test]
fn test_alias_map_simple_use() {
    let f = parse("use foo::bar;");
    let map = gather_alias_map(&f);
    assert_eq!(map.get("bar"), Some(&relative(&["foo", "bar"])));
}

#[test]
fn test_alias_map_rename() {
    let f = parse("use foo::bar as baz;");
    let map = gather_alias_map(&f);
    assert_eq!(map.get("baz"), Some(&relative(&["foo", "bar"])));
    assert!(
        !map.contains_key("bar"),
        "renamed origin must not leak into the alias map"
    );
}

#[test]
fn test_alias_map_nested_group() {
    let f = parse("use foo::{bar, baz};");
    let map = gather_alias_map(&f);
    assert_eq!(map.get("bar"), Some(&relative(&["foo", "bar"])));
    assert_eq!(map.get("baz"), Some(&relative(&["foo", "baz"])));
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
    assert_eq!(map.get("foo"), Some(&relative(&["foo"])));
    assert_eq!(map.get("bar"), Some(&relative(&["foo", "bar"])));
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
        Some(&relative(&["foo"])),
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
        Some(&relative(&["crate", "app", "RlmSession"]))
    );
}

#[test]
fn test_alias_map_multiple_top_level_uses() {
    let f = parse(
        "use foo::A;\n\
         use bar::B;",
    );
    let map = gather_alias_map(&f);
    assert_eq!(map.get("A"), Some(&relative(&["foo", "A"])));
    assert_eq!(map.get("B"), Some(&relative(&["bar", "B"])));
}

#[test]
fn test_alias_map_absolute_path_preserves_leading_colon() {
    // `use ::ext::Foo;` is Rust 2018+ extern-root path. The alias map
    // MUST remember the leading colon — without it, downstream
    // canonicalisation can silently rewrite `ext::Foo` into
    // `crate::ext::Foo` when a same-named workspace module exists.
    let f = parse("use ::ext::Foo;");
    let map = gather_alias_map(&f);
    assert_eq!(map.get("Foo"), Some(&absolute(&["ext", "Foo"])));
}

#[test]
fn test_alias_map_absolute_path_renamed_preserves_leading_colon() {
    let f = parse("use ::ext::Foo as Local;");
    let map = gather_alias_map(&f);
    assert_eq!(map.get("Local"), Some(&absolute(&["ext", "Foo"])));
    assert!(!map.contains_key("Foo"));
}

#[test]
fn test_alias_map_mixed_absolute_and_relative() {
    // Two consecutive `use` items: one relative, one absolute. Each
    // entry must carry its OWN absolute-root bit.
    let f = parse(
        "use foo::A;\n\
         use ::bar::B;",
    );
    let map = gather_alias_map(&f);
    assert_eq!(map.get("A"), Some(&relative(&["foo", "A"])));
    assert_eq!(map.get("B"), Some(&absolute(&["bar", "B"])));
}
