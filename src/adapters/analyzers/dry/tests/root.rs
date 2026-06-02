use crate::adapters::analyzers::dry::*;
use crate::config::sections::DuplicatesConfig;

fn parse(code: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(code).expect("parse failed");
    vec![("test.rs".to_string(), code.to_string(), syntax)]
}

/// Collect function hashes for `code` under a config with the given
/// thresholds and trait-impl flag. (DRY always runs on test code.)
fn hashes_of(
    code: &str,
    min_tokens: usize,
    min_lines: usize,
    ignore_trait_impls: bool,
) -> Vec<FunctionHashEntry> {
    let config = DuplicatesConfig {
        min_tokens,
        min_lines,
        ignore_trait_impls,
        ..DuplicatesConfig::default()
    };
    collect_function_hashes(&parse(code), &config)
}

// (label, code, min_tokens, min_lines, ignore_trait_impls, expected_len,
// expected_qualified)
type HashCase = (
    &'static str,
    &'static str,
    usize,
    usize,
    bool,
    usize,
    Option<&'static str>,
);

const HASH_CASES: &[HashCase] = &[
    ("empty source", "", 30, 5, true, 0, None),
    (
        "tiny fn filtered by default min_tokens",
        "fn tiny() { let x = 1; }",
        30,
        5,
        true,
        0,
        None,
    ),
    (
        "large free fn included",
        r#"
            fn big_fn() {
                let a = 1;
                let b = 2;
                let c = a + b;
                let d = c * a;
                let e = d - b;
                let f = e + c;
                let g = f * d;
                let h = g - e;
                let i = h + f;
                let j = i * g;
            }
            "#,
        5,
        1,
        true,
        1,
        Some("big_fn"),
    ),
    (
        "cfg(test) fn included (DRY runs on test code)",
        r#"
            #[cfg(test)]
            mod tests {
                fn helper() {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
            "#,
        5,
        1,
        true,
        1,
        None,
    ),
    (
        "impl method included with qualified name",
        r#"
            struct Foo;
            impl Foo {
                fn method(&self) {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
            "#,
        5,
        1,
        true,
        1,
        Some("Foo::method"),
    ),
    (
        "trait-impl method excluded when ignore_trait_impls",
        r#"
            trait Bar { fn do_thing(&self); }
            struct Foo;
            impl Bar for Foo {
                fn do_thing(&self) {
                    let a = 1; let b = 2; let c = a + b;
                    let d = c * a; let e = d - b; let f = e + c;
                }
            }
            "#,
        5,
        1,
        true,
        0,
        None,
    ),
];

#[test]
fn collect_function_hashes_inclusion() {
    // `collect_function_hashes` includes a fn only when it clears the token
    // threshold and isn't filtered out: tiny fns drop (default min_tokens=30);
    // trait-impl methods drop iff `ignore_trait_impls`. Test code is always
    // included (the `ignore_tests` toggle was removed in v1.4.0).
    for (label, code, min_tokens, min_lines, ignore_trait_impls, expected_len, qualified) in
        HASH_CASES
    {
        let entries = hashes_of(code, *min_tokens, *min_lines, *ignore_trait_impls);
        assert_eq!(entries.len(), *expected_len, "case {label}: len");
        if let Some(q) = qualified {
            assert_eq!(
                &entries[0].qualified_name, q,
                "case {label}: qualified_name"
            );
        }
    }
}

#[test]
fn test_has_cfg_test_positive() {
    let code = "#[cfg(test)] mod tests {}";
    let file = syn::parse_file(code).unwrap();
    if let syn::Item::Mod(m) = &file.items[0] {
        assert!(has_cfg_test(&m.attrs));
    }
}

#[test]
fn test_has_cfg_test_negative() {
    let code = "#[cfg(feature = \"foo\")] mod feature_mod {}";
    let file = syn::parse_file(code).unwrap();
    if let syn::Item::Mod(m) = &file.items[0] {
        assert!(!has_cfg_test(&m.attrs));
    }
}

#[test]
fn test_has_test_attr() {
    let code = "#[test] fn test_something() {}";
    let file = syn::parse_file(code).unwrap();
    if let syn::Item::Fn(f) = &file.items[0] {
        assert!(has_test_attr(&f.attrs));
    }
}

#[test]
fn test_collect_declared_functions_basic() {
    let code = "fn foo() {} fn bar() {} fn main() {}";
    let parsed = parse(code);
    let declared = collect_declared_functions(&parsed);
    assert_eq!(declared.len(), 3);
    assert!(declared.iter().any(|d| d.name == "main" && d.is_main));
    assert!(declared.iter().any(|d| d.name == "foo" && !d.is_main));
}

/// Whether `fn_name` in `code` is a declared test function (`is_test`).
fn declared_is_test(code: &str, fn_name: &str) -> bool {
    collect_declared_functions(&parse(code))
        .into_iter()
        .find(|d| d.name == fn_name)
        .unwrap_or_else(|| panic!("fn {fn_name} not found"))
        .is_test
}

#[test]
fn declared_function_is_test_classification() {
    // `collect_declared_functions` marks fns inside `#[cfg(test)] mod` /
    // `#[cfg(test)] impl` as tests (false for production fns alongside them).
    // (label, code, fn_name, expected_is_test)
    const CFG_TEST_MOD: &str = r#"
        fn production() {}
        #[cfg(test)]
        mod tests {
            fn helper() {}
            #[test]
            fn test_something() {}
        }
    "#;
    const CFG_TEST_IMPL: &str = r#"
        pub struct Foo;

        #[cfg(test)]
        impl Foo {
            fn test_helper(&self) -> bool { true }
            pub fn another_helper() -> i32 { 42 }
        }
    "#;
    let cases: &[(&str, &str, &str, bool)] = &[
        (
            "production fn outside cfg(test)",
            CFG_TEST_MOD,
            "production",
            false,
        ),
        (
            "plain helper in cfg(test) mod",
            CFG_TEST_MOD,
            "helper",
            true,
        ),
        (
            "#[test] fn in cfg(test) mod",
            CFG_TEST_MOD,
            "test_something",
            true,
        ),
        (
            "method in cfg(test) impl",
            CFG_TEST_IMPL,
            "test_helper",
            true,
        ),
        (
            "pub method in cfg(test) impl",
            CFG_TEST_IMPL,
            "another_helper",
            true,
        ),
    ];
    for (label, code, fn_name, expected) in cases {
        assert_eq!(declared_is_test(code, fn_name), *expected, "case {label}");
    }
}

#[test]
fn test_collect_declared_functions_trait_impl() {
    let code = r#"
        trait Foo { fn bar(&self); }
        struct S;
        impl Foo for S {
            fn bar(&self) {}
        }
    "#;
    let parsed = parse(code);
    let declared = collect_declared_functions(&parsed);
    let bar = declared.iter().find(|d| d.name == "bar").unwrap();
    assert!(bar.is_trait_impl);
}

#[test]
fn test_collect_declared_functions_allow_dead_code() {
    let code = "#[allow(dead_code)] fn unused() {}";
    let parsed = parse(code);
    let declared = collect_declared_functions(&parsed);
    assert_eq!(declared.len(), 1);
    assert!(declared[0].has_allow_dead_code);
}

#[test]
fn test_collect_declared_functions_allow_dead_code_in_list() {
    // `#[allow(..., dead_code, ...)]` is the common multi-lint form.
    // A previous parse-as-Ident implementation silently missed this.
    let code =
        "#[allow(unused_variables, dead_code, unused_imports)] fn unused(x: i32) { let y = 1; }";
    let parsed = parse(code);
    let declared = collect_declared_functions(&parsed);
    assert_eq!(declared.len(), 1);
    assert!(
        declared[0].has_allow_dead_code,
        "dead_code inside a multi-lint allow list must be detected"
    );
}

#[test]
fn test_collect_declared_functions_allow_list_without_dead_code() {
    // Control: a list that doesn't include dead_code must not match.
    let code = "#[allow(unused_variables, unused_imports)] fn unused(x: i32) {}";
    let parsed = parse(code);
    let declared = collect_declared_functions(&parsed);
    assert_eq!(declared.len(), 1);
    assert!(!declared[0].has_allow_dead_code);
}
