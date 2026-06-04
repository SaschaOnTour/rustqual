use super::*;

#[test]
fn test_fn_with_test_attr_is_test() {
    let code = r#"
        fn helper() {}
        #[test]
        fn my_test() {
            helper();
            if true {}
        }
    "#;
    let results = parse_and_analyze(code);
    let test_fn = results.iter().find(|f| f.name == "my_test").unwrap();
    assert!(
        test_fn.is_test,
        "Function with #[test] should have is_test=true"
    );
}

// `is_test` marks fns inside `#[cfg(test)] mod` / `#[cfg(test)] impl` (and is
// false for production fns and regular impl methods alongside them).
// (label, code, fn_name, expected_is_test)
const CFG_TEST_MOD: &str = r#"
        fn production_fn() {}
        #[cfg(test)]
        mod tests {
            fn test_helper() {}
        }
    "#;
const CFG_TEST_IMPL: &str = r#"
        pub struct Config { pub name: String }

        impl Config {
            pub fn new(name: String) -> Self { Self { name } }
        }

        #[cfg(test)]
        impl Config {
            fn test_helper(&self) -> bool { true }
            pub fn another_helper() -> i32 { if true { 1 } else { 2 } }
        }
    "#;

const IS_TEST_CASES: &[(&str, &str, &str, bool)] = &[
    (
        "production fn outside cfg(test)",
        CFG_TEST_MOD,
        "production_fn",
        false,
    ),
    (
        "fn inside #[cfg(test)] mod",
        CFG_TEST_MOD,
        "test_helper",
        true,
    ),
    (
        "plain regular fn",
        "fn regular() { if true {} }",
        "regular",
        false,
    ),
    (
        "method inside #[cfg(test)] impl",
        CFG_TEST_IMPL,
        "test_helper",
        true,
    ),
    (
        "pub method inside #[cfg(test)] impl",
        CFG_TEST_IMPL,
        "another_helper",
        true,
    ),
    (
        "method in a regular impl alongside it",
        CFG_TEST_IMPL,
        "new",
        false,
    ),
];

#[test]
fn is_test_classification() {
    for (label, code, fn_name, expected) in IS_TEST_CASES {
        assert_eq!(is_test_of(code, fn_name), *expected, "case {label}");
    }
}

// ---------------------------------------------------------------
// Bug 2: Method-call type resolution tests
// ---------------------------------------------------------------
