use crate::adapters::analyzers::architecture::matcher::find_item_kind_matches;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("test fixture must parse")
}

fn find(src: &str, kinds: &[&str]) -> Vec<MatchLocation> {
    let ast = parse(src);
    let owned: Vec<String> = kinds.iter().map(|k| (*k).to_string()).collect();
    find_item_kind_matches("fixture.rs", &ast, &owned)
}

fn kinds(hits: &[MatchLocation]) -> Vec<&'static str> {
    hits.iter()
        .filter_map(|h| match &h.kind {
            ViolationKind::ItemKind { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect()
}

fn names(hits: &[MatchLocation]) -> Vec<String> {
    hits.iter()
        .filter_map(|h| match &h.kind {
            ViolationKind::ItemKind { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

// ── clean baselines ───────────────────────────────────────────────────

#[test]
fn clean_file_no_matches() {
    let src = r#"
        pub fn sync_function() {}
        pub struct Foo;
        impl Foo { pub fn bar(&self) {} }
    "#;
    assert!(find(src, &["async_fn", "unsafe_fn", "unsafe_impl"]).is_empty());
}

#[test]
fn unrequested_kind_never_matches() {
    let src = "pub async fn foo() {}";
    assert!(find(src, &["unsafe_fn"]).is_empty());
}

// ── async_fn ──────────────────────────────────────────────────────────

#[test]
fn async_fn_top_level() {
    let src = "pub async fn foo() {}";
    let hits = find(src, &["async_fn"]);
    assert_eq!(hits.len(), 1);
    assert_eq!(kinds(&hits), vec!["async_fn"]);
    assert_eq!(names(&hits), vec!["foo"]);
}

#[test]
fn async_fn_inside_impl() {
    let src = r#"
        pub struct S;
        impl S { pub async fn work(&self) {} }
    "#;
    let hits = find(src, &["async_fn"]);
    assert_eq!(hits.len(), 1);
    assert_eq!(names(&hits), vec!["work"]);
}

#[test]
fn async_fn_inside_nested_mod() {
    let src = r#"
        pub mod inner { pub async fn foo() {} }
    "#;
    let hits = find(src, &["async_fn"]);
    assert_eq!(hits.len(), 1);
}

// ── unsafe_fn ─────────────────────────────────────────────────────────

#[test]
fn unsafe_fn_top_level() {
    let src = "pub unsafe fn bar() {}";
    let hits = find(src, &["unsafe_fn"]);
    assert_eq!(kinds(&hits), vec!["unsafe_fn"]);
    assert_eq!(names(&hits), vec!["bar"]);
}

#[test]
fn unsafe_fn_inside_impl() {
    let src = r#"
        pub struct S;
        impl S { pub unsafe fn dangerous(&self) {} }
    "#;
    let hits = find(src, &["unsafe_fn"]);
    assert_eq!(hits.len(), 1);
}

// ── unsafe_impl ───────────────────────────────────────────────────────

#[test]
fn unsafe_impl_matched() {
    let src = r#"
        pub struct S;
        unsafe impl Send for S {}
    "#;
    let hits = find(src, &["unsafe_impl"]);
    assert_eq!(kinds(&hits), vec!["unsafe_impl"]);
}

#[test]
fn regular_impl_not_matched() {
    let src = r#"
        pub struct S;
        impl Default for S { fn default() -> Self { S } }
    "#;
    assert!(find(src, &["unsafe_impl"]).is_empty());
}

// ── static_mut ────────────────────────────────────────────────────────

#[test]
fn static_mut_matched() {
    let src = "pub static mut COUNTER: usize = 0;";
    let hits = find(src, &["static_mut"]);
    assert_eq!(kinds(&hits), vec!["static_mut"]);
    assert_eq!(names(&hits), vec!["COUNTER"]);
}

#[test]
fn immutable_static_not_matched() {
    let src = "pub static COUNTER: usize = 0;";
    assert!(find(src, &["static_mut"]).is_empty());
}

// ── extern_c_block ────────────────────────────────────────────────────

#[test]
fn extern_c_block_matched() {
    let src = r#"
        extern "C" {
            pub fn printf(fmt: *const u8, ...) -> i32;
        }
    "#;
    let hits = find(src, &["extern_c_block"]);
    assert_eq!(kinds(&hits), vec!["extern_c_block"]);
}

#[test]
fn extern_rust_block_not_matched() {
    // `extern "Rust"` is semantically the default ABI; still count it as
    // a foreign block. The rule is `extern_c_block` so any extern block
    // triggers.
    let src = r#"
        extern "Rust" { fn foo(); }
    "#;
    let hits = find(src, &["extern_c_block"]);
    assert_eq!(
        hits.len(),
        1,
        "any extern block counts as extern_c_block today"
    );
}

// ── inline_cfg_test_module ────────────────────────────────────────────

#[test]
fn inline_cfg_test_mod_matched() {
    let src = r#"
        #[cfg(test)]
        mod tests { fn t() {} }
    "#;
    let hits = find(src, &["inline_cfg_test_module"]);
    assert_eq!(kinds(&hits), vec!["inline_cfg_test_module"]);
    assert_eq!(names(&hits), vec!["tests"]);
}

#[test]
fn cfg_test_mod_declaration_without_body_not_matched() {
    let src = r#"
        #[cfg(test)]
        mod tests;
    "#;
    // Declaration-only: companion file, OK.
    assert!(find(src, &["inline_cfg_test_module"]).is_empty());
}

#[test]
fn mod_without_cfg_test_not_matched() {
    let src = "pub mod utils { fn helper() {} }";
    assert!(find(src, &["inline_cfg_test_module"]).is_empty());
}

// ── top_level_cfg_test_item ───────────────────────────────────────────

#[test]
fn top_level_cfg_test_fn_matched() {
    let src = r#"
        #[cfg(test)]
        fn helper_fixture() {}
    "#;
    let hits = find(src, &["top_level_cfg_test_item"]);
    assert_eq!(kinds(&hits), vec!["top_level_cfg_test_item"]);
    assert_eq!(names(&hits), vec!["helper_fixture"]);
}

#[test]
fn top_level_cfg_test_const_matched() {
    let src = r#"
        #[cfg(test)]
        const FIXTURE: usize = 10;
    "#;
    let hits = find(src, &["top_level_cfg_test_item"]);
    assert_eq!(kinds(&hits), vec!["top_level_cfg_test_item"]);
}

#[test]
fn top_level_cfg_test_impl_matched() {
    let src = r#"
        pub struct S;
        #[cfg(test)]
        impl S { fn only_in_test(&self) {} }
    "#;
    let hits = find(src, &["top_level_cfg_test_item"]);
    assert_eq!(kinds(&hits), vec!["top_level_cfg_test_item"]);
}

#[test]
fn top_level_cfg_test_ignores_mod_tests() {
    // `#[cfg(test)] mod tests { ... }` is the `inline_cfg_test_module`
    // rule's territory; don't double-report it here.
    let src = r#"
        #[cfg(test)]
        mod tests { fn t() {} }
    "#;
    assert!(find(src, &["top_level_cfg_test_item"]).is_empty());
}

#[test]
fn nested_cfg_test_fn_inside_mod_not_top_level() {
    let src = r#"
        pub mod inner {
            #[cfg(test)]
            fn fixture() {}
        }
    "#;
    assert!(find(src, &["top_level_cfg_test_item"]).is_empty());
}

// ── multiple kinds requested together ────────────────────────────────

#[test]
fn multiple_kinds_each_checked() {
    let src = r#"
        pub async fn a() {}
        pub unsafe fn b() {}
        pub static mut C: i32 = 0;
    "#;
    let hits = find(src, &["async_fn", "unsafe_fn", "static_mut"]);
    assert_eq!(hits.len(), 3);
    let ks: std::collections::HashSet<&str> = kinds(&hits).into_iter().collect();
    assert!(ks.contains("async_fn"));
    assert!(ks.contains("unsafe_fn"));
    assert!(ks.contains("static_mut"));
}

#[test]
fn unknown_kind_silently_ignored() {
    let src = "pub async fn foo() {}";
    // Bogus string shouldn't error the matcher; async_fn still fires.
    let hits = find(src, &["bogus_kind", "async_fn"]);
    assert_eq!(hits.len(), 1);
}
