use crate::adapters::analyzers::tq::assertions::*;
use crate::adapters::analyzers::tq::{TqWarning, TqWarningKind};
use syn::visit::Visit;

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
    let extras = vec!["verify".to_string()];
    let warnings = parse_and_detect_with_extras(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn test_custom() {
                verify!(result.is_ok());
            }
        }
        "#,
        &extras,
    );
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
