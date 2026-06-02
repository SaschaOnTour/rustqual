use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};
use proptest::prelude::*;

/// Parse a single free function and return its attributes.
fn fn_attrs(code: &str) -> Vec<syn::Attribute> {
    let file = syn::parse_file(code).expect("parse failed");
    match &file.items[0] {
        syn::Item::Fn(f) => f.attrs.clone(),
        _ => panic!("expected a free fn"),
    }
}

#[test]
fn bare_test_attribute_recognized() {
    assert!(has_test_attr(&fn_attrs("#[test] fn t() {}")));
}

#[test]
fn framework_test_attributes_recognized() {
    // Any attribute whose path ends in `test` is a test entry point:
    // the async-runtime test macros all follow this shape.
    assert!(
        has_test_attr(&fn_attrs(
            "#[tokio::test(flavor = \"multi_thread\")] async fn t() {}"
        )),
        "#[tokio::test] must be recognised"
    );
    assert!(
        has_test_attr(&fn_attrs("#[async_std::test] async fn t() {}")),
        "#[async_std::test] must be recognised"
    );
    assert!(
        has_test_attr(&fn_attrs("#[googletest::test] fn t() {}")),
        "#[googletest::test] must be recognised"
    );
    // The renamed-macro frameworks that do not end in `test`:
    assert!(
        has_test_attr(&fn_attrs("#[rstest] fn t() {}")),
        "#[rstest] must be recognised"
    );
    assert!(
        has_test_attr(&fn_attrs("#[test_case(1)] fn t() {}")),
        "#[test_case] must be recognised"
    );
    assert!(
        has_test_attr(&fn_attrs("#[quickcheck] fn prop(x: u8) -> bool { true }")),
        "#[quickcheck] must be recognised"
    );
}

#[test]
fn non_test_attributes_not_recognized() {
    assert!(!has_test_attr(&fn_attrs("#[inline] fn t() {}")));
    assert!(!has_test_attr(&fn_attrs("#[allow(dead_code)] fn t() {}")));
    // `#[cfg(test)]` is a separate signal handled by has_cfg_test, not
    // has_test_attr — keep the two predicates distinct.
    assert!(!has_test_attr(&fn_attrs("#[cfg(test)] fn t() {}")));
}

#[test]
fn cfg_test_attribute_still_distinct() {
    assert!(has_cfg_test(&fn_attrs("#[cfg(test)] fn t() {}")));
    assert!(!has_cfg_test(&fn_attrs("#[test] fn t() {}")));
}

/// Canonical attribute matrix: `(decorated fn, has_test_attr?, has_cfg_test?)`.
/// One row per recognised entry-point form + edge spellings + the cfg(test) /
/// test distinction. A missed variant here is exactly the v1.3.0 bug class.
const ATTR_MATRIX: &[(&str, bool, bool)] = &[
    ("#[test] fn t() {}", true, false),
    ("#[::core::prelude::v1::test] fn t() {}", true, false),
    ("#[tokio::test] async fn t() {}", true, false),
    (
        "#[tokio::test(flavor = \"multi_thread\")] async fn t() {}",
        true,
        false,
    ),
    ("#[async_std::test] async fn t() {}", true, false),
    ("#[googletest::test] fn t() {}", true, false),
    ("#[rstest] fn t() {}", true, false),
    ("#[rstest::rstest] fn t() {}", true, false),
    ("#[test_case(1)] fn t() {}", true, false),
    ("#[quickcheck] fn p(x: u8) -> bool { true }", true, false),
    ("#[cfg(test)] fn t() {}", false, true),
    ("#[cfg(test)]\n#[test] fn t() {}", true, true),
    ("#[inline] fn t() {}", false, false),
    ("#[allow(dead_code)] fn t() {}", false, false),
    ("#[doc = \"x\"] fn t() {}", false, false),
    ("fn t() {}", false, false),
];

#[test]
fn test_attribute_variant_matrix() {
    for (code, want_test, want_cfg) in ATTR_MATRIX {
        let attrs = fn_attrs(code);
        assert_eq!(has_test_attr(&attrs), *want_test, "has_test_attr: {code}");
        assert_eq!(has_cfg_test(&attrs), *want_cfg, "has_cfg_test: {code}");
    }
}

proptest! {
    /// `has_test_attr` recognises a `…::test` entry point under ANY module-path
    /// prefix and depth (`#[a::b::test]`), not just the bare `#[test]` — the
    /// "last path segment is `test`" contract, checked generatively.
    #[test]
    fn qualified_test_path_always_recognized(
        prefix in prop::collection::vec(
            prop::sample::select(vec!["tokio", "async_std", "rt", "crate", "a", "b"]),
            0..4usize,
        )
    ) {
        let path: String = prefix.iter().map(|s| format!("{s}::")).collect();
        let code = format!("#[{path}test] fn t() {{}}");
        prop_assert!(
            has_test_attr(&fn_attrs(&code)),
            "qualified test path not recognised: {code}"
        );
    }

    /// A single non-test attribute ident is never a test entry point. Draws from
    /// real attribute names so every case parses.
    #[test]
    fn non_test_attribute_never_recognized(
        attr in prop::sample::select(vec![
            "inline", "cold", "must_use", "no_mangle", "non_exhaustive", "ignore",
        ])
    ) {
        let code = format!("#[{attr}] fn t() {{}}");
        prop_assert!(!has_test_attr(&fn_attrs(&code)), "false positive: {code}");
    }
}
