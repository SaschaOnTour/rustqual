use crate::adapters::shared::cfg_test::has_test_attr;
use crate::adapters::shared::macro_expansion::expand_test_macros;

/// Run the expansion pre-pass over a single source file and return the
/// rewritten syntax tree.
fn expand(src: &str) -> syn::File {
    let file = syn::parse_file(src).expect("parse failed");
    let mut parsed = vec![("t.rs".to_string(), src.to_string(), file)];
    expand_test_macros(&mut parsed);
    parsed.into_iter().next().unwrap().2
}

/// Collect every `fn` reachable in the file as `(name, is_test_attr)`,
/// descending into inline modules.
fn collect_fns(items: &[syn::Item]) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for item in items {
        match item {
            syn::Item::Fn(f) => out.push((f.sig.ident.to_string(), has_test_attr(&f.attrs))),
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    out.extend(collect_fns(inner));
                }
            }
            _ => {}
        }
    }
    out
}

fn has_macro_item(items: &[syn::Item]) -> bool {
    items.iter().any(|i| matches!(i, syn::Item::Macro(_)))
}

#[test]
fn proptest_block_surfaces_inner_test_fn() {
    let file = expand(
        r#"
#[cfg(test)]
mod tests {
    proptest! {
        #[test]
        fn prop_under_100(x in 0..100u32) {
            assert!(x < 100);
        }
    }
}
"#,
    );
    let fns = collect_fns(&file.items);
    assert!(
        fns.iter().any(|(n, t)| n == "prop_under_100" && *t),
        "proptest! inner fn must surface as a test fn, got {fns:?}"
    );
}

#[test]
fn proptest_block_surfaces_multiple_fns() {
    let file = expand(
        r#"
proptest! {
    #[test]
    fn first(x in 0..1u8) { let _ = x; }
    #[test]
    fn second(y in 0..1u8) { let _ = y; }
}
"#,
    );
    let names: Vec<String> = collect_fns(&file.items)
        .into_iter()
        .filter(|(_, t)| *t)
        .map(|(n, _)| n)
        .collect();
    assert!(
        names.contains(&"first".to_string()) && names.contains(&"second".to_string()),
        "both proptest fns must surface, got {names:?}"
    );
}

#[test]
fn proptest_config_attribute_does_not_block_expansion() {
    let file = expand(
        r#"
proptest! {
    #![proptest_config(ProptestConfig { cases: 10, ..ProptestConfig::default() })]
    #[test]
    fn with_config(x in 0..10u32) {
        assert!(x < 10);
    }
}
"#,
    );
    let fns = collect_fns(&file.items);
    assert!(
        fns.iter().any(|(n, t)| n == "with_config" && *t),
        "leading #![proptest_config(..)] must not block expansion, got {fns:?}"
    );
}

#[test]
fn quickcheck_block_marks_fn_as_test() {
    // quickcheck! does not write `#[test]` in the source — the macro adds
    // it. The expansion must synthesise it so the fn is recognised.
    let file = expand(
        r#"
mod tests {
    quickcheck! {
        fn commutes(x: u32, y: u32) -> bool {
            x.wrapping_add(y) == y.wrapping_add(x)
        }
    }
}
"#,
    );
    let fns = collect_fns(&file.items);
    assert!(
        fns.iter().any(|(n, t)| n == "commutes" && *t),
        "quickcheck! fn must surface and be marked as a test, got {fns:?}"
    );
}

#[test]
fn non_test_macro_left_untouched() {
    let file = expand(
        r#"
lazy_static! {
    static ref VALUE: u32 = 5;
}
"#,
    );
    assert!(
        collect_fns(&file.items).is_empty(),
        "a non-test macro must not produce any fn"
    );
    assert!(
        has_macro_item(&file.items),
        "the original non-test macro item must be preserved"
    );
}

#[test]
fn unparseable_test_macro_falls_back_to_opaque() {
    let file = expand(
        r#"
proptest! {
    this is @@ not a valid fn body
}
"#,
    );
    assert!(
        collect_fns(&file.items).is_empty(),
        "unparseable body must yield no fns"
    );
    assert!(
        has_macro_item(&file.items),
        "unparseable test macro must be kept verbatim (no regression)"
    );
}
