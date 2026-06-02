use super::*;

// ── Deprecated-attribute detection (v1.2.1) ────────────────────────

fn deprecated_for_layer<'ast>(
    by_layer: &std::collections::HashMap<String, Vec<PubFnInfo<'ast>>>,
    layer: &str,
) -> HashMap<String, bool> {
    by_layer
        .get(layer)
        .map(|fns| {
            fns.iter()
                .map(|f| (f.fn_name.clone(), f.deprecated))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn pub_fn_records_deprecated_attribute_bare() {
    let file = parse(
        r#"
        #[deprecated]
        pub fn cmd_old() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    let dep = deprecated_for_layer(&by_layer, "cli");
    assert_eq!(dep.get("cmd_old"), Some(&true));
}

#[test]
fn pub_fn_records_deprecated_with_message() {
    let file = parse(
        r#"
        #[deprecated = "use cmd_new instead"]
        pub fn cmd_old() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    assert_eq!(
        deprecated_for_layer(&by_layer, "cli").get("cmd_old"),
        Some(&true)
    );
}

#[test]
fn pub_fn_records_deprecated_with_args() {
    let file = parse(
        r#"
        #[deprecated(since = "1.0", note = "use cmd_new")]
        pub fn cmd_old() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    assert_eq!(
        deprecated_for_layer(&by_layer, "cli").get("cmd_old"),
        Some(&true)
    );
}

#[test]
fn pub_fn_no_attribute_not_deprecated() {
    let file = parse(
        r#"
        pub fn cmd_active() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    assert_eq!(
        deprecated_for_layer(&by_layer, "cli").get("cmd_active"),
        Some(&false)
    );
}

#[test]
fn pub_fn_other_attribute_not_deprecated() {
    // `#[allow(unused)]` must NOT be misidentified as deprecation.
    let file = parse(
        r#"
        #[allow(unused)]
        pub fn cmd_active() {}
        "#,
    );
    let files = vec![("src/cli/handlers.rs", &file)];
    let by_layer = pub_fns_by_layer(&files);
    assert_eq!(
        deprecated_for_layer(&by_layer, "cli").get("cmd_active"),
        Some(&false)
    );
}
