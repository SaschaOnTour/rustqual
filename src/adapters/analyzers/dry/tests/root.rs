use crate::adapters::analyzers::dry::*;
use crate::config::sections::DuplicatesConfig;

fn parse(code: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(code).expect("parse failed");
    vec![("test.rs".to_string(), code.to_string(), syntax)]
}

#[test]
fn test_collect_function_hashes_empty() {
    let parsed = parse("");
    let config = DuplicatesConfig::default();
    let entries = collect_function_hashes(&parsed, &config);
    assert!(entries.is_empty());
}

#[test]
fn test_collect_function_hashes_small_function_excluded() {
    // A tiny function should be excluded by min_tokens
    let parsed = parse("fn tiny() { let x = 1; }");
    let config = DuplicatesConfig::default(); // min_tokens = 30
    let entries = collect_function_hashes(&parsed, &config);
    assert!(entries.is_empty(), "Small function should be filtered out");
}

#[test]
fn test_collect_function_hashes_large_function_included() {
    // A larger function with many tokens
    let code = r#"
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
    "#;
    let parsed = parse(code);
    let config = DuplicatesConfig {
        min_tokens: 5, // Lower threshold for test
        min_lines: 1,
        ..DuplicatesConfig::default()
    };
    let entries = collect_function_hashes(&parsed, &config);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "big_fn");
}

#[test]
fn test_collect_function_hashes_test_excluded() {
    let code = r#"
        #[cfg(test)]
        mod tests {
            fn helper() {
                let a = 1; let b = 2; let c = a + b;
                let d = c * a; let e = d - b; let f = e + c;
            }
        }
    "#;
    let parsed = parse(code);
    let config = DuplicatesConfig {
        min_tokens: 5,
        min_lines: 1,
        ignore_tests: true,
        ..DuplicatesConfig::default()
    };
    let entries = collect_function_hashes(&parsed, &config);
    assert!(entries.is_empty(), "Test functions should be excluded");
}

#[test]
fn test_collect_function_hashes_test_included_when_not_ignored() {
    let code = r#"
        #[cfg(test)]
        mod tests {
            fn helper() {
                let a = 1; let b = 2; let c = a + b;
                let d = c * a; let e = d - b; let f = e + c;
            }
        }
    "#;
    let parsed = parse(code);
    let config = DuplicatesConfig {
        min_tokens: 5,
        min_lines: 1,
        ignore_tests: false,
        ..DuplicatesConfig::default()
    };
    let entries = collect_function_hashes(&parsed, &config);
    assert_eq!(entries.len(), 1, "Test functions should be included");
}

#[test]
fn test_collect_function_hashes_impl_method() {
    let code = r#"
        struct Foo;
        impl Foo {
            fn method(&self) {
                let a = 1; let b = 2; let c = a + b;
                let d = c * a; let e = d - b; let f = e + c;
            }
        }
    "#;
    let parsed = parse(code);
    let config = DuplicatesConfig {
        min_tokens: 5,
        min_lines: 1,
        ..DuplicatesConfig::default()
    };
    let entries = collect_function_hashes(&parsed, &config);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].qualified_name, "Foo::method");
}

#[test]
fn test_collect_function_hashes_trait_impl_excluded() {
    let code = r#"
        trait Bar { fn do_thing(&self); }
        struct Foo;
        impl Bar for Foo {
            fn do_thing(&self) {
                let a = 1; let b = 2; let c = a + b;
                let d = c * a; let e = d - b; let f = e + c;
            }
        }
    "#;
    let parsed = parse(code);
    let config = DuplicatesConfig {
        min_tokens: 5,
        min_lines: 1,
        ignore_trait_impls: true,
        ..DuplicatesConfig::default()
    };
    let entries = collect_function_hashes(&parsed, &config);
    assert!(entries.is_empty(), "Trait impl methods should be excluded");
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

#[test]
fn test_collect_declared_functions_test_context() {
    let code = r#"
        fn production() {}
        #[cfg(test)]
        mod tests {
            fn helper() {}
            #[test]
            fn test_something() {}
        }
    "#;
    let parsed = parse(code);
    let declared = collect_declared_functions(&parsed);
    let prod = declared.iter().find(|d| d.name == "production").unwrap();
    assert!(!prod.is_test);
    let helper = declared.iter().find(|d| d.name == "helper").unwrap();
    assert!(helper.is_test);
    let test_fn = declared
        .iter()
        .find(|d| d.name == "test_something")
        .unwrap();
    assert!(test_fn.is_test);
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
fn test_cfg_test_impl_methods_are_test() {
    let code = r#"
        pub struct Foo;

        #[cfg(test)]
        impl Foo {
            fn test_helper(&self) -> bool { true }
            pub fn another_helper() -> i32 { 42 }
        }
    "#;
    let parsed = parse(code);
    let declared = collect_declared_functions(&parsed);

    let helper = declared.iter().find(|d| d.name == "test_helper").unwrap();
    assert!(
        helper.is_test,
        "Method inside #[cfg(test)] impl should have is_test=true"
    );

    let another = declared
        .iter()
        .find(|d| d.name == "another_helper")
        .unwrap();
    assert!(
        another.is_test,
        "Pub method inside #[cfg(test)] impl should have is_test=true"
    );
}
