use crate::adapters::shared::cfg_test::{has_cfg_test, has_test_attr};

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
