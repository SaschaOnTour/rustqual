use crate::adapters::analyzers::tq::assertions::*;
use crate::adapters::analyzers::tq::{TqWarning, TqWarningKind};

fn parse_and_detect(source: &str) -> Vec<TqWarning> {
    parse_and_detect_with_extras(source, &[])
}

fn parse_and_detect_with_extras(source: &str, extras: &[String]) -> Vec<TqWarning> {
    let syntax = syn::parse_file(source).expect("test source");
    let parsed = vec![("test.rs".to_string(), source.to_string(), syntax)];
    detect_assertion_free_tests(&parsed, extras)
}

#[test]
fn test_with_assert_no_warning() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_something() {
                assert!(true);
            }
        }
        "#,
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_with_assert_eq_no_warning() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_something() {
                assert_eq!(1, 1);
            }
        }
        "#,
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_without_assertion_emits_warning() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_something() {
                let x = 42;
            }
        }
        "#,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind, TqWarningKind::NoAssertion);
}

#[test]
fn test_should_panic_with_panic_no_warning() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            #[should_panic]
            fn test_something() {
                panic!("expected");
            }
        }
        "#,
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_should_panic_without_panic_emits_warning() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            #[should_panic]
            fn test_something() {
                let x = 42;
            }
        }
        "#,
    );
    assert_eq!(warnings.len(), 1);
}

#[test]
fn test_empty_test_emits_warning() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_empty() {}
        }
        "#,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].function_name, "test_empty");
}

#[test]
fn test_debug_assert_no_warning() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_something() {
                debug_assert!(true);
            }
        }
        "#,
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_prop_assert_no_warning() {
    // proptest's `prop_assert!` / `prop_assert_eq!` are assertions too — a
    // property test that uses only them must not be flagged TQ_NO_ASSERT.
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn prop_something() {
                prop_assert_eq!(1, 1);
            }
        }
        "#,
    );
    assert!(
        warnings.is_empty(),
        "prop_assert_eq! must count as an assertion: {warnings:?}"
    );
}

#[test]
fn quickcheck_bool_property_no_warning() {
    // The macro-expansion pre-pass surfaces `quickcheck! { fn p(x: u8) -> bool
    // { x < 100 } }` as a synthetic `#[test] fn p() -> bool { x < 100 }` (params
    // dropped). The boolean RETURN is the property's oracle — quickcheck checks
    // it holds for every generated input — so a bare-boolean body with no
    // assertion macro and no call must NOT be flagged TQ-001 assertion-free.
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn small() -> bool { x < 100 }
        }
        "#,
    );
    assert!(
        warnings.is_empty(),
        "a `-> bool` property's boolean return is its assertion: {warnings:?}"
    );
}

#[test]
fn unit_return_without_assertion_still_warns() {
    // Guard the `-> bool` leniency above: it must be specific to a boolean
    // oracle. A normal unit-returning test with no assertion/call is still a
    // TQ-001 NoAssertion — the bool exemption must not leak to `()` tests.
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn nothing() { let _x = 1 < 2; }
        }
        "#,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind, TqWarningKind::NoAssertion);
}

#[test]
fn test_assert_ne_no_warning() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_something() {
                assert_ne!(1, 2);
            }
        }
        "#,
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_non_test_function_ignored() {
    let warnings = parse_and_detect(
        r#"
        fn not_a_test() {
            let x = 42;
        }
        "#,
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_assert_prefixed_custom_macro_no_warning() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_approx() {
                assert_relative_eq!(1.0, 1.0001, epsilon = 0.01);
            }
        }
        "#,
    );
    assert!(
        warnings.is_empty(),
        "assert_relative_eq! should be recognized by prefix"
    );
}

#[test]
fn test_extra_assertion_macro_config() {
    // The config must MATTER: `verify!` is not a built-in assertion (no
    // `assert`/`debug_assert` prefix), so without the extras it triggers
    // TQ-001; only the `extra_assertion_macros = ["verify"]` config makes it
    // count. Asserting only the configured-on case would pass even if every
    // macro were blanket-accepted — pin both sides.
    let source = r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_custom() {
                verify!(result.is_ok());
            }
        }
        "#;
    let without = parse_and_detect(source);
    assert_eq!(
        without.len(),
        1,
        "verify! is NOT a built-in assertion → TQ-001 without the config"
    );
    assert_eq!(without[0].kind, TqWarningKind::NoAssertion);

    let extras = vec!["verify".to_string()];
    let warnings = parse_and_detect_with_extras(source, &extras);
    assert!(
        warnings.is_empty(),
        "verify! in extra_assertion_macros should be recognized"
    );
}

#[test]
fn test_no_assertion_still_warns() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_nothing() {
                let _ = 42;
            }
        }
        "#,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind, TqWarningKind::NoAssertion);
}

#[test]
fn test_multiple_tests_mixed() {
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_good() {
                assert!(true);
            }
            #[test]
            fn test_bad() {
                let x = 42;
            }
        }
        "#,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].function_name, "test_bad");
}

#[test]
fn non_assertion_macro_alone_emits_warning() {
    // A non-assertion macro (`println!`) is not an assertion and is not a call,
    // so a test whose only statement is one must still be flagged — guards
    // `is_assertion_macro` against blanket-accepting every macro.
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_something() {
                println!("hi");
            }
        }
        "#,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind, TqWarningKind::NoAssertion);
}

#[test]
fn expression_position_assertion_no_warning() {
    // An assertion in *expression* position (`let _ = assert_eq!(…)`) is an
    // `Expr::Macro`, not a `Stmt::Macro` — exercises `visit_expr_macro`.
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_something() {
                let _ = assert_eq!(1, 1);
            }
        }
        "#,
    );
    assert!(warnings.is_empty());
}

#[test]
fn should_panic_with_expression_position_panic_no_warning() {
    // A `panic!` in expression position drives the `== "panic"` check inside
    // `visit_expr_macro`; combined with `#[should_panic]` it is a valid oracle.
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            #[should_panic]
            fn test_something() {
                let _ = panic!("boom");
            }
        }
        "#,
    );
    assert!(warnings.is_empty());
}

#[test]
fn method_call_only_no_warning() {
    // A test whose only effect is a *method* call (no free-function call, no
    // assertion) is still exercising code — guards `visit_expr_method_call`.
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_something() {
                1u32.count_ones();
            }
        }
        "#,
    );
    assert!(warnings.is_empty());
}

#[test]
fn panic_without_should_panic_attr_emits_warning() {
    // A `panic!` only counts as an oracle when the test is `#[should_panic]`.
    // Without that attribute, a panic-only test has no real assertion and must
    // be flagged — guards `has_should_panic_attr` against returning `true`.
    let warnings = parse_and_detect(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_something() {
                panic!("boom");
            }
        }
        "#,
    );
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].kind, TqWarningKind::NoAssertion);
}
