use crate::adapters::analyzers::dry::dead_code::*;
use crate::config::Config;
use std::collections::HashSet;

fn parse(code: &str) -> Vec<(String, String, syn::File)> {
    let syntax = syn::parse_file(code).expect("parse failed");
    vec![("test.rs".to_string(), code.to_string(), syntax)]
}

/// Parse a two-file workspace (`parent` + `child`) into a `parsed` vec.
fn parse2(
    parent_path: &str,
    parent_code: &str,
    child_path: &str,
    child_code: &str,
) -> Vec<(String, String, syn::File)> {
    vec![
        (
            parent_path.to_string(),
            parent_code.to_string(),
            syn::parse_file(parent_code).expect("parse parent"),
        ),
        (
            child_path.to_string(),
            child_code.to_string(),
            syn::parse_file(child_code).expect("parse child"),
        ),
    ]
}

/// `(production_calls, test_calls)` collected from `code` (single file).
fn collected_calls(code: &str) -> (HashSet<String>, HashSet<String>) {
    let parsed = parse(code);
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(&parsed);
    let calls = collect_all_calls(&parsed, &cfg_test_files);
    // The helper keeps the pre-split shape: a re-export is production usage for
    // every consumer but TQ-003.
    let mut production = calls.refs.production;
    production.extend(calls.reexported);
    (production, calls.refs.tests)
}

/// Run dead-code detection over `parsed` with the default config and no
/// api/test-helper line markers.
fn dead_code_warnings(parsed: &[(String, String, syn::File)]) -> Vec<DeadCodeWarning> {
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(parsed);
    detect_dead_code(
        parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &cfg_test_files,
    )
}

#[test]
fn test_detect_dead_code_empty() {
    let parsed = parse("");
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(warnings.is_empty());
}

#[test]
fn test_uncalled_function_detected() {
    let code = r#"
        fn called_fn() { let x = 1; }
        fn caller() { called_fn(); }
        fn never_called() { let y = 2; }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    let uncalled: Vec<_> = warnings
        .iter()
        .filter(|w| w.kind == DeadCodeKind::Uncalled)
        .collect();
    assert!(
        uncalled.iter().any(|w| w.function_name == "never_called"),
        "never_called should be flagged as uncalled"
    );
    assert!(
        !uncalled.iter().any(|w| w.function_name == "called_fn"),
        "called_fn should not be flagged"
    );
}

#[test]
fn test_called_function_not_flagged() {
    let code = r#"
        fn helper() { let x = 1; }
        fn main() { helper(); }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "helper"),
        "called function should not be flagged"
    );
}

#[test]
fn test_main_excluded_from_dead_code() {
    let code = "fn main() {}";
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "main"),
        "main should never be flagged"
    );
}

#[test]
fn test_test_function_excluded() {
    let code = r#"
        #[test]
        fn test_something() { let x = 1; }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "test_something"),
        "test functions should be excluded"
    );
}

#[test]
fn test_trait_impl_excluded() {
    let code = r#"
        trait Foo { fn bar(&self); }
        struct S;
        impl Foo for S {
            fn bar(&self) {}
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "bar"),
        "trait impl methods should be excluded"
    );
}

#[test]
fn test_allow_dead_code_excluded() {
    let code = r#"
        #[allow(dead_code)]
        fn intentionally_unused() { let x = 1; }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.function_name == "intentionally_unused"),
        "Functions with #[allow(dead_code)] should be excluded"
    );
}

#[test]
fn test_test_only_function_detected() {
    let code = r#"
        fn helper() { let x = 1; }
        fn production() { let y = 2; }
        #[cfg(test)]
        mod tests {
            use super::*;
            #[test]
            fn test_it() { helper(); }
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    let test_only: Vec<_> = warnings
        .iter()
        .filter(|w| w.kind == DeadCodeKind::TestOnly)
        .collect();
    assert!(
        test_only.iter().any(|w| w.function_name == "helper"),
        "helper called only from tests should be flagged as test-only"
    );
}

#[test]
fn testonly_suggestion_mentions_qual_api_and_test_helper() {
    // The actionable path for integration-test helpers living in src/
    // is either `// qual:api` (if truly public) or `// qual:test_helper`
    // (if only the test binary uses them). The suggestion text must
    // mention both so the finding is self-documenting.
    let code = r#"
        pub fn helper() { let x = 1; }
        #[cfg(test)]
        mod tests {
            use super::helper;
            #[test]
            fn t() { helper(); }
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    let testonly = warnings
        .iter()
        .find(|w| w.kind == DeadCodeKind::TestOnly)
        .expect("helper should be flagged test-only");
    assert!(
        testonly.suggestion.contains("qual:api")
            && testonly.suggestion.contains("qual:test_helper"),
        "testonly suggestion should mention both escape hatches, got: {:?}",
        testonly.suggestion
    );
}

#[test]
fn test_dead_code_always_runs_when_called_directly() {
    // The detect_dead_code flag is checked by the pipeline caller, not by
    // detect_dead_code itself (to maintain IOSP Integration compliance).
    let code = r#"
        fn never_called() { let x = 1; }
    "#;
    let parsed = parse(code);
    let mut config = Config::default();
    config.duplicates.detect_dead_code = false;
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.is_empty(),
        "detect_dead_code runs regardless — pipeline guards the config flag"
    );
}

#[test]
fn test_method_call_detected() {
    let code = r#"
        struct S;
        impl S {
            fn helper(&self) { let x = 1; }
            fn caller(&self) { self.helper(); }
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "helper"),
        "Method called via self.helper() should not be flagged"
    );
}

#[test]
fn test_function_reference_as_call_argument() {
    let code = r#"
        fn some_fn(x: i32) -> i32 { x + 1 }
        fn caller() {
            let items = vec![1, 2, 3];
            let _: Vec<_> = items.into_iter().map(some_fn).collect();
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "some_fn"),
        "Function passed as argument to map() should be detected as called"
    );
}

#[test]
fn test_function_reference_as_method_argument() {
    let code = r#"
        fn process(x: i32) { let _ = x; }
        fn caller() {
            let items = vec![1, 2, 3];
            items.iter().for_each(process);
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "process"),
        "Function passed as argument to for_each() should be detected as called"
    );
}

#[test]
fn test_qualified_function_reference_as_argument() {
    let code = r#"
        mod report {
            pub fn print_item(x: &i32) { let _ = x; }
        }
        fn caller() {
            let items = vec![1, 2, 3];
            items.iter().for_each(report::print_item);
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "print_item"),
        "Qualified function reference (module::fn) should be detected as called"
    );
}

#[test]
fn test_qualified_call_detected() {
    let code = r#"
        struct Config;
        impl Config {
            fn load() -> Self { Config }
        }
        fn main() { let c = Config::load(); }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "load"),
        "Config::load() should be detected as called"
    );
}

#[test]
fn test_pub_use_reexport_not_dead_code() {
    let code = r#"
        mod foo { pub fn do_work() { let x = 1; } }
        pub use foo::do_work;
        fn main() {}
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "do_work"),
        "pub use re-exported function should not be flagged as dead code"
    );
}

#[test]
fn test_pub_use_rename_not_dead_code() {
    let code = r#"
        mod foo { pub fn do_work() { let x = 1; } }
        pub use foo::do_work as perform_work;
        fn main() {}
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "do_work"),
        "pub use rename re-export should record original name, not alias"
    );
}

#[test]
fn test_pub_use_group_reexport_not_dead_code() {
    let code = r#"
        mod foo {
            pub fn bar() { let x = 1; }
            pub fn baz() { let y = 2; }
        }
        pub use foo::{bar, baz};
        fn main() {}
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "bar"),
        "grouped pub use re-export: bar should not be flagged"
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "baz"),
        "grouped pub use re-export: baz should not be flagged"
    );
}

#[test]
fn test_private_use_does_not_count_as_reexport() {
    let code = r#"
        mod foo { pub fn helper() { let x = 1; } }
        use foo::helper;
        fn main() {}
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        warnings.iter().any(|w| w.function_name == "helper"),
        "private use import (no call) should still be flagged as uncalled"
    );
}

#[test]
fn integration_test_entry_point_in_crate_tests_dir_not_dead_code() {
    // A `#[tokio::test]` entry point under a workspace crate's `tests/`
    // directory is an integration-test root that cargo compiles and
    // runs. It has no in-crate callers but must NOT be flagged as dead
    // code — the same treatment already applied to `#[test]` fns.
    let code = r#"
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn five_oh_two_twice_then_success_under_default_policy() { let x = 1; }
    "#;
    let parsed = vec![(
        "crates/sv-utility-retry/tests/integration.rs".to_string(),
        code.to_string(),
        syn::parse_file(code).expect("parse failed"),
    )];
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(&parsed);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &cfg_test_files,
    );
    assert!(
        warnings.is_empty(),
        "integration-test entry point must not be flagged as dead code: {warnings:?}"
    );
}

#[test]
fn cfg_test_mod_child_files_not_flagged() {
    // A function in an externally-declared `#[cfg(test)] mod X;` child file
    // is test code and must not be flagged as dead, across the three
    // module-resolution shapes: flat (`helpers.rs`), dir (`helpers/mod.rs`),
    // and non-mod parent (`foo.rs` → `foo/`). (label, parent_path,
    // parent_code, child_path, child_code, fn_not_flagged)
    let cases: &[(&str, &str, &str, &str, &str, &str)] = &[
        (
            "flat helpers.rs child",
            "src/mod.rs",
            r#"
            fn production_fn() { let x = 1; }
            #[cfg(test)]
            mod helpers;
            "#,
            "src/helpers.rs",
            "pub fn test_helper() { let x = 1; }",
            "test_helper",
        ),
        (
            "dir helpers/mod.rs child",
            "src/foo/mod.rs",
            r#"
            fn prod() { let x = 1; }
            #[cfg(test)]
            mod helpers;
            "#,
            "src/foo/helpers/mod.rs",
            "pub fn test_util() { let x = 1; }",
            "test_util",
        ),
        (
            "non-mod parent foo.rs → foo/",
            "src/foo.rs",
            r#"
            fn prod() { let x = 1; }
            #[cfg(test)]
            mod test_utils;
            "#,
            "src/foo/test_utils.rs",
            "pub fn helper() { let x = 1; }",
            "helper",
        ),
    ];
    for (label, parent_path, parent_code, child_path, child_code, fn_name) in cases {
        let warnings =
            dead_code_warnings(&parse2(parent_path, parent_code, child_path, child_code));
        assert!(
            !warnings.iter().any(|w| w.function_name == *fn_name),
            "case {label}: {fn_name} in a #[cfg(test)] mod child must not be flagged; got {warnings:?}"
        );
    }
}

#[test]
fn test_cfg_test_mod_calls_classified_as_test() {
    // Parent declares #[cfg(test)] mod helpers; externally
    let parent_code = r#"
        fn used_by_test_helpers() { let x = 1; }
        fn used_by_production() { let y = 2; }
        fn caller() { used_by_production(); }
        #[cfg(test)]
        mod helpers;
    "#;
    // Child file calls used_by_test_helpers — should be a test call
    let child_code = r#"
        pub fn test_helper() { super::used_by_test_helpers(); }
    "#;
    let parent_ast = syn::parse_file(parent_code).expect("parse parent");
    let child_ast = syn::parse_file(child_code).expect("parse child");
    let parsed = vec![
        (
            "src/lib.rs".to_string(),
            parent_code.to_string(),
            parent_ast,
        ),
        (
            "src/helpers.rs".to_string(),
            child_code.to_string(),
            child_ast,
        ),
    ];
    let cfg_test_files =
        crate::adapters::shared::cfg_test_files::collect_cfg_test_file_paths(&parsed);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &cfg_test_files,
    );
    // used_by_test_helpers is only called from cfg(test) file → TestOnly
    let test_only: Vec<_> = warnings
        .iter()
        .filter(|w| w.kind == DeadCodeKind::TestOnly)
        .collect();
    assert!(
        test_only
            .iter()
            .any(|w| w.function_name == "used_by_test_helpers"),
        "Function called only from cfg(test) file should be flagged as test-only"
    );
    // used_by_production is called from production code → not flagged
    assert!(
        !warnings
            .iter()
            .any(|w| w.function_name == "used_by_production"),
        "Function called from production should not be flagged"
    );
}

// ── Serde attribute tests ────────────────────────────────

#[test]
fn test_serde_deserialize_with_not_dead_code() {
    let code = r#"
        fn custom_de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
            let v: i32 = serde::Deserialize::deserialize(d)?;
            Ok(v)
        }
        #[derive(serde::Deserialize)]
        struct Foo {
            #[serde(deserialize_with = "custom_de")]
            value: i32,
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "custom_de"),
        "Function referenced by #[serde(deserialize_with)] should not be flagged"
    );
}

#[test]
fn test_serde_serialize_with_not_dead_code() {
    let code = r#"
        fn custom_ser<S: serde::Serializer>(v: &i32, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_i32(*v)
        }
        #[derive(serde::Serialize)]
        struct Foo {
            #[serde(serialize_with = "custom_ser")]
            value: i32,
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "custom_ser"),
        "Function referenced by #[serde(serialize_with)] should not be flagged"
    );
}

#[test]
fn test_serde_default_fn_not_dead_code() {
    let code = r#"
        fn default_val() -> i32 { 42 }
        #[derive(serde::Deserialize)]
        struct Foo {
            #[serde(default = "default_val")]
            value: i32,
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "default_val"),
        "Function referenced by #[serde(default = \"fn\")] should not be flagged"
    );
}

#[test]
fn test_serde_qualified_path_not_dead_code() {
    let code = r#"
        mod helpers {
            pub fn custom_de<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
                let v: i32 = serde::Deserialize::deserialize(d)?;
                Ok(v)
            }
        }
        #[derive(serde::Deserialize)]
        struct Foo {
            #[serde(deserialize_with = "helpers::custom_de")]
            value: i32,
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "custom_de"),
        "Qualified serde fn ref (helpers::custom_de) should not be flagged"
    );
}

#[test]
fn test_serde_with_module_not_dead_code() {
    let code = r#"
        mod my_format {
            pub fn serialize<S: serde::Serializer>(_v: &i32, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_i32(0)
            }
            pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
                let v: i32 = serde::Deserialize::deserialize(d)?;
                Ok(v)
            }
        }
        struct Foo {
            #[serde(with = "my_format")]
            value: i32,
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    // "serialize" and "deserialize" are universal methods, so they'd be excluded anyway
    // but let's make sure neither triggers
    assert!(
        !warnings.iter().any(|w| w.function_name == "serialize"),
        "Function referenced via #[serde(with)] should not be flagged"
    );
}

#[test]
fn test_serde_default_without_value_ignored() {
    // #[serde(default)] without = "fn" should not crash or extract anything
    let code = r#"
        fn unused_fn() { let x = 1; }
        #[derive(serde::Deserialize)]
        struct Foo {
            #[serde(default)]
            value: i32,
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        warnings.iter().any(|w| w.function_name == "unused_fn"),
        "Unrelated unused function should still be flagged"
    );
}

#[test]
fn test_serde_default_fn_cross_file_not_dead_code() {
    // File A defines default functions, File B uses them via #[serde(default = "...")]
    let code_a = r#"
        pub fn default_true() -> bool { true }
        pub fn default_adx_period() -> u32 { 14 }
    "#;
    let code_b = r#"
        #[derive(serde::Deserialize)]
        struct Config {
            #[serde(default = "default_true")]
            enabled: bool,
            #[serde(default = "default_adx_period")]
            adx_period: u32,
        }
    "#;
    let ast_a = syn::parse_file(code_a).expect("parse code_a");
    let ast_b = syn::parse_file(code_b).expect("parse code_b");
    let parsed = vec![
        (
            "src/config_defaults.rs".to_string(),
            code_a.to_string(),
            ast_a,
        ),
        ("src/config.rs".to_string(), code_b.to_string(), ast_b),
    ];
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "default_true"),
        "default_true referenced via #[serde(default)] in another file should not be flagged"
    );
    assert!(
        !warnings
            .iter()
            .any(|w| w.function_name == "default_adx_period"),
        "default_adx_period referenced via #[serde(default)] in another file should not be flagged"
    );
}

#[test]
fn test_serde_default_fn_realistic_pattern() {
    let code = r#"
        fn default_true() -> bool { true }
        fn default_false() -> bool { false }
        fn default_period() -> u32 { 14 }
        fn default_threshold() -> f64 { 0.5 }

        #[derive(serde::Deserialize)]
        struct IndicatorConfig {
            #[serde(default = "default_true")]
            enabled: bool,
            #[serde(default = "default_false")]
            verbose: bool,
            #[serde(default = "default_period")]
            period: u32,
            #[serde(default = "default_threshold")]
            threshold: f64,
        }
    "#;
    let parsed = parse(code);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    let flagged: Vec<&str> = warnings.iter().map(|w| w.function_name.as_str()).collect();
    assert!(
        !flagged.contains(&"default_true"),
        "default_true should not be flagged, got: {flagged:?}"
    );
    assert!(
        !flagged.contains(&"default_false"),
        "default_false should not be flagged, got: {flagged:?}"
    );
    assert!(
        !flagged.contains(&"default_period"),
        "default_period should not be flagged, got: {flagged:?}"
    );
    assert!(
        !flagged.contains(&"default_threshold"),
        "default_threshold should not be flagged, got: {flagged:?}"
    );
}

// `collect_all_calls` must see calls in various syntactic forms and route them
// to the production vs test set by scope: calls inside `assert!` / `assert_eq!`
// within a `#[test]` count as TEST calls; a `&fn` passed as an argument and a
// bare fn name in a struct field count as PRODUCTION calls.
// (label, code, in_test_scope, callee)
const CALL_FORM_CASES: &[(&str, &str, bool, &str)] = &[
    (
        "call inside assert!() in a #[test]",
        r#"
            fn helper() -> bool { true }
            #[cfg(test)]
            mod tests {
                use super::*;
                #[test]
                fn test_it() {
                    assert!(helper());
                }
            }
            "#,
        true,
        "helper",
    ),
    (
        "call inside assert_eq!() in a #[test]",
        r#"
            fn compute() -> usize { 42 }
            #[cfg(test)]
            mod tests {
                use super::*;
                #[test]
                fn test_it() {
                    assert_eq!(compute(), 42);
                }
            }
            "#,
        true,
        "compute",
    ),
    (
        "&fn passed as a closure argument",
        r#"
            fn format_item(s: &str) -> String { s.to_string() }

            fn process(items: &[&str], transform: &dyn Fn(&str) -> String) {
                items.iter().for_each(|i| { transform(i); });
            }

            fn run() {
                process(&["a"], &format_item);
            }
            "#,
        false,
        "format_item",
    ),
    (
        "bare fn name in a struct field",
        r#"
            struct Config { handler: fn() -> i32 }
            fn my_handler() -> i32 { 42 }
            fn setup() -> Config {
                Config { handler: my_handler }
            }
            "#,
        false,
        "my_handler",
    ),
];

#[test]
fn collect_all_calls_recognizes_call_forms() {
    for (label, code, in_test_scope, callee) in CALL_FORM_CASES {
        let (prod_calls, test_calls) = collected_calls(code);
        let set = if *in_test_scope {
            &test_calls
        } else {
            &prod_calls
        };
        assert!(
            set.contains(*callee),
            "case {label}: callee `{callee}` not found in {set:?}"
        );
    }
}

#[test]
fn test_collect_cfg_test_file_paths_inline_mod_ignored() {
    // Inline #[cfg(test)] mod (with body) should NOT produce entries
    let code = r#"
        #[cfg(test)]
        mod tests {
            fn helper() {}
        }
    "#;
    let ast = syn::parse_file(code).unwrap();
    let parsed = vec![("src/lib.rs".to_string(), code.to_string(), ast)];
    let result = collect_cfg_test_file_paths(&parsed);
    assert!(
        result.is_empty(),
        "Inline cfg(test) mod should not produce cfg-test file entries"
    );
}

// ── API marker tests ─────────────────────────────────────────

#[test]
fn test_api_function_excluded_from_dead_code() {
    let code = r#"
        // qual:api
        pub fn public_api() { let x = 1; }

        // spacer to move internal_unused outside annotation window
        // another spacer line
        fn internal_unused() { let y = 2; }
    "#;
    let parsed = parse(code);
    let mut api_lines = std::collections::HashMap::new();
    api_lines.insert(
        "test.rs".to_string(),
        [2usize]
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
    );
    let warnings = detect_dead_code(
        &parsed,
        &api_lines,
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    let names: Vec<&str> = warnings.iter().map(|w| w.function_name.as_str()).collect();
    assert!(
        !names.contains(&"public_api"),
        "API-marked function should be excluded"
    );
    assert!(
        names.contains(&"internal_unused"),
        "Non-API function should still be flagged"
    );
}

#[test]
fn test_api_does_not_count_as_suppression() {
    // Verify parse_suppression returns None for qual:api
    assert!(crate::findings::parse_suppression(1, "// qual:api").is_none());
}

// ── Test-helper marker tests ─────────────────────────────────

#[test]
fn test_helper_marker_suppresses_testonly_dead_code() {
    // Helper is called from a test module but not production. Without
    // the marker it would produce a TestOnly finding; with the marker
    // it is silenced (but other checks would still apply).
    let code = r#"
        // qual:test_helper
        pub fn shared_asserter(x: i32) {
            let _ = x + 1;
        }

        #[cfg(test)]
        mod tests {
            use super::shared_asserter;
            #[test]
            fn t1() { shared_asserter(1); }
        }
    "#;
    let parsed = parse(code);
    let mut test_helper_lines = std::collections::HashMap::new();
    test_helper_lines.insert(
        "test.rs".to_string(),
        [2usize]
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
    );
    let cfg_test_files = collect_cfg_test_file_paths(&parsed);
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &test_helper_lines,
        &cfg_test_files,
    );
    let names: Vec<&str> = warnings.iter().map(|w| w.function_name.as_str()).collect();
    assert!(
        !names.contains(&"shared_asserter"),
        "test-helper-marked function should be excluded from dead code, got {names:?}"
    );
}

#[test]
fn test_helper_marker_does_not_suppress_uncalled() {
    // `// qual:test_helper` narrowly silences the TestOnly variant
    // of dead code — not the Uncalled variant. A function marked as
    // test_helper but with no callers anywhere (including tests) is
    // still worth flagging: the marker is likely stale or placed on
    // the wrong function. This is the counterpart to
    // `test_helper_marker_suppresses_testonly_dead_code` — both
    // together specify the full semantics.
    let code = r#"
        // qual:test_helper
        pub fn marked_but_not_called() { let _ = 1; }

        // spacer to move unmarked out of annotation window
        // another spacer line
        fn unmarked_and_uncalled() { let _ = 2; }
    "#;
    let parsed = parse(code);
    let mut test_helper_lines = std::collections::HashMap::new();
    test_helper_lines.insert(
        "test.rs".to_string(),
        [2usize]
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
    );
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &test_helper_lines,
        &std::collections::HashSet::new(),
    );
    let uncalled: Vec<&str> = warnings
        .iter()
        .filter(|w| w.kind == DeadCodeKind::Uncalled)
        .map(|w| w.function_name.as_str())
        .collect();
    assert!(
        uncalled.contains(&"marked_but_not_called"),
        "test_helper on a function with no callers must still flag as Uncalled, got {uncalled:?}"
    );
    assert!(
        uncalled.contains(&"unmarked_and_uncalled"),
        "unmarked uncalled function must still flag, got {uncalled:?}"
    );
}

// ── Trait-Blanket-Dispatch reachability ─────────────────────────────
//
// Reproduces the v1.2.2 setup that triggered the original concern: a
// helper function only reachable via `<T as Reporter>::render → <T as
// ReporterImpl>::publish → helper`. The fear was that the analyzer
// can't trace the blanket-impl indirection and would mark `helper` as
// Uncalled or TestOnly. The visitor in `call_targets.rs` records
// method-name calls (`self.publish()` → "publish") and free-function
// calls (`helper()` → "helper") regardless of where they appear, so
// both sides of the chain are captured by name and the helper stays
// production-reachable.

/// Three-file workspace mirroring the v1.2.2 incident: `ports/reporter.rs`
/// (trait defs + blanket `impl<T: ReporterImpl> Reporter for T`),
/// `sarif/rules.rs` (the helper that was flagged), and `sarif/mod.rs` (the
/// concrete `SarifReporter` + a `#[cfg(test)]` module calling the helper via a
/// pub-API wrapper — the thing that made it look test-only).
fn blanket_dispatch_parsed() -> Vec<(String, String, syn::File)> {
    let pf = |path: &str, src: &str| {
        (
            path.to_string(),
            String::new(),
            syn::parse_file(src).expect("parse fixture"),
        )
    };
    vec![
        pf(
            "src/ports/reporter.rs",
            r#"
            pub trait ReporterImpl {
                type Output;
                fn publish(&self) -> Self::Output;
            }
            pub trait Reporter {
                type Output;
                fn render(&self) -> Self::Output;
            }
            impl<T: ReporterImpl> Reporter for T {
                type Output = T::Output;
                fn render(&self) -> Self::Output { self.publish() }
            }
            "#,
        ),
        pf(
            "src/adapters/report/sarif/rules.rs",
            r#"
            pub(super) fn sarif_rules() -> Vec<String> {
                vec![String::from("rule")]
            }
            "#,
        ),
        pf(
            "src/adapters/report/sarif/mod.rs",
            r#"
            use super::rules::sarif_rules;
            pub struct SarifReporter;
            impl ReporterImpl for SarifReporter {
                type Output = String;
                fn publish(&self) -> Self::Output {
                    let _r = sarif_rules();
                    String::new()
                }
            }
            pub fn build_sarif_string() -> String {
                let r = SarifReporter;
                r.render()
            }
            #[cfg(test)]
            mod tests {
                use super::build_sarif_string;
                #[test]
                fn it() { let _ = build_sarif_string(); }
            }
            "#,
        ),
    ]
}

// Production path `<SarifReporter as Reporter>::render → …ReporterImpl::publish
// → sarif_rules()` reaches the helper through a trait-blanket impl. If the
// analyzer can't trace that, it sees only the `#[cfg(test)]` caller and wrongly
// concludes `sarif_rules` is TestOnly. It must NOT be flagged dead-code.
#[test]
fn helper_reached_via_trait_blanket_dispatch_is_not_dead_code() {
    let parsed = blanket_dispatch_parsed();
    let warnings = detect_dead_code(
        &parsed,
        &std::collections::HashMap::new(),
        &std::collections::HashMap::new(),
        &std::collections::HashSet::new(),
    );
    assert!(
        !warnings.iter().any(|w| w.function_name == "sarif_rules"),
        "sarif_rules is reached via trait-blanket dispatch and must not be flagged dead-code, got: {:?}",
        warnings.iter().map(|w| (&w.function_name, &w.kind)).collect::<Vec<_>>()
    );
}

#[test]
fn fn_called_only_in_repeat_macro_arg_not_dead_code() {
    // `caller` invokes `helper()` ONLY inside a `vec![_; n]` *repeat* macro.
    // Before the macro-token fix, `visit_macro` (call_targets.rs) re-parsed the
    // body only as a comma-separated expr list; the `;` separator failed that
    // parse and EVERY call edge in the macro was dropped, so `helper` was
    // wrongly reported dead. `recover_exprs`' block fallback now recovers it.
    //
    // The comma-expr form `vec![helper()]` was already handled; the fix covers
    // the forms that don't parse as an expr list — repeat (`;`) and block
    // bodies. See the bound-local control below for the non-macro baseline.
    let code = r#"
        fn helper() -> i32 { 42 }
        fn caller() -> Vec<i32> { vec![helper(); 3] }
        #[cfg(test)]
        mod tests {
            #[test]
            fn t() { let _ = super::caller(); }
        }
    "#;
    let parsed = parse(code);
    let warnings = dead_code_warnings(&parsed);
    assert!(
        !warnings.iter().any(|w| w.function_name == "helper"),
        "helper() called inside a vec![_; n] repeat macro must not be flagged dead, got {warnings:?}"
    );
}

#[test]
fn component_referenced_only_in_dsl_macro_not_dead_code() {
    // Dioxus-style: `LiveLogViewer` is a production component referenced only
    // inside an `rsx!` DSL body whose nested elements + attributes parse as
    // none of expr-list / block / expr. `recover_exprs` yields nothing, so the
    // raw `idents_in_tokens` fallback must still register the reference —
    // otherwise the component is wrongly flagged dead (a real-world rsx! symptom).
    let code = r#"
        fn LiveLogViewer() { let _ = 1; }
        fn app() { rsx! { div { class: "log", LiveLogViewer {} } }; }
        fn main() { app(); }
    "#;
    let parsed = parse(code);
    let warnings = dead_code_warnings(&parsed);
    assert!(
        !warnings.iter().any(|w| w.function_name == "LiveLogViewer"),
        "component referenced only inside an rsx! DSL macro must not be dead, got {warnings:?}"
    );
}

#[test]
fn fn_named_like_a_lowercase_dsl_tag_is_still_dead_code() {
    // `view` is an unused production fn whose name also appears as a lowercase
    // DSL element tag (`view { .. }`) in a rendered body. A lowercase tag is not
    // a component/struct, so the positional harvest must NOT record it — `view`
    // stays flagged dead. Guards the UpperCamelCase brace-group gate.
    let code = r#"
        fn view() { let _ = 1; }
        fn app() { rsx! { view { class: "x", Widget {} } }; }
        fn main() { app(); }
    "#;
    let parsed = parse(code);
    let warnings = dead_code_warnings(&parsed);
    assert!(
        warnings.iter().any(|w| w.function_name == "view"),
        "a fn matching only a lowercase DSL tag must still be flagged dead, got {warnings:?}"
    );
}

#[test]
fn fn_named_like_a_dsl_prop_key_is_still_dead_code() {
    // Negative control for the positional raw harvest: `render_label` appears in
    // the rsx! body ONLY as a prop key (`Widget { render_label: "x" }`), not in
    // call/construction position. It must NOT be harvested as a call, so the
    // genuinely-uncalled `render_label` stays flagged dead — guarding against
    // the over-collection false-negative the raw-ident fallback would have had.
    let code = r#"
        fn render_label() { let _ = 1; }
        fn app() { rsx! { Widget { render_label: "x" } }; }
        fn main() { app(); }
    "#;
    let parsed = parse(code);
    let warnings = dead_code_warnings(&parsed);
    assert!(
        warnings.iter().any(|w| w.function_name == "render_label"),
        "a fn matching only a DSL prop key must still be flagged dead, got {warnings:?}"
    );
}

#[test]
fn fn_called_via_bound_local_not_dead_code_control() {
    // Control for `fn_called_only_in_repeat_macro_arg_not_dead_code`: the same
    // call routed through a bound local (`let h = helper(); vec![h]`) is a
    // plain AST call expression — not buried in macro tokens — so the edge is
    // recorded and `helper` is correctly NOT dead. Isolates the bug to macro
    // token streams, not to `vec!` itself.
    let code = r#"
        fn helper() -> i32 { 42 }
        fn caller() -> Vec<i32> { let h = helper(); vec![h] }
        #[cfg(test)]
        mod tests {
            #[test]
            fn t() { let _ = super::caller(); }
        }
    "#;
    let parsed = parse(code);
    let warnings = dead_code_warnings(&parsed);
    assert!(
        !warnings.iter().any(|w| w.function_name == "helper"),
        "helper() via a bound local must not be flagged dead, got {warnings:?}"
    );
}

#[test]
fn test_only_called_fn_is_not_uncalled() {
    // A production fn called only from tests is TestOnly, NOT Uncalled — guards
    // the `!test_calls.contains(qualified)` term of `find_uncalled` (a `||`
    // there would wrongly report it as uncalled).
    let parsed =
        parse("fn helper() { let _ = 1; } #[cfg(test)] mod tests { #[test] fn t() { helper(); } }");
    let warnings = dead_code_warnings(&parsed);
    assert!(
        !warnings
            .iter()
            .any(|w| w.function_name == "helper" && matches!(w.kind, DeadCodeKind::Uncalled)),
        "a test-only fn must not be reported as Uncalled, got {warnings:?}"
    );
}

#[test]
fn allow_dead_code_is_inherited_by_functions_too() {
    // DRY-002 reads the same attribute and must inherit it the same way, or the
    // two dead-code checks would disagree about one file.
    let cfg_test_files = HashSet::new();
    let empty = std::collections::HashMap::new();
    let found = detect_dead_code(
        &parse("#![allow(dead_code)]\nfn intentional() {}"),
        &empty,
        &empty,
        &cfg_test_files,
    );
    assert!(
        found.is_empty(),
        "inherited allow covers functions: {found:?}"
    );

    let in_mod = detect_dead_code(
        &parse("#[allow(dead_code)]\nmod generated { fn generated() {} }"),
        &empty,
        &empty,
        &cfg_test_files,
    );
    assert!(
        in_mod.is_empty(),
        "module-level allow covers functions: {in_mod:?}"
    );
}

#[test]
fn allow_dead_code_is_inherited_from_an_enclosing_impl() {
    // The common generated-code shape: one attribute on the impl block rather
    // than one per method.
    let empty = std::collections::HashMap::new();
    let found = detect_dead_code(
        &parse("struct Generated;\n#[allow(dead_code)]\nimpl Generated { fn helper() {} }"),
        &empty,
        &empty,
        &HashSet::new(),
    );
    assert!(
        found.iter().all(|w| w.function_name != "helper"),
        "impl-level allow covers its methods: {found:?}"
    );
}

#[test]
fn an_ffi_export_is_not_dead_code() {
    // The counterpart of the DRY-006 case: an exported function's caller is a
    // linker, so no call site exists in the workspace by design. Both spellings
    // count, including Rust 2024's `#[unsafe(no_mangle)]`.
    let bare = dead_code_warnings(&parse(
        "#[no_mangle]\npub extern \"C\" fn plugin_entry() {}",
    ));
    assert!(bare.is_empty(), "{bare:?}");
    let edition_2024 = dead_code_warnings(&parse(
        "#[unsafe(no_mangle)]\npub extern \"C\" fn plugin_entry() {}",
    ));
    assert!(edition_2024.is_empty(), "{edition_2024:?}");
}

#[test]
fn allow_dead_code_is_inherited_across_the_file_boundary() {
    // A lint level covers everything below it and does not stop at the file a
    // module happens to live in. DRY-002 read only the declaring file's own
    // attributes, so a function the author had excused one level up was still
    // reported — a false finding by the rule this check documents.
    let excused = parse2(
        "src/lib.rs",
        "#![allow(dead_code)]\npub mod inner;",
        "src/inner.rs",
        "pub fn unused() {}",
    );
    assert!(dead_code_warnings(&excused).is_empty());
    let on_the_declaration = parse2(
        "src/lib.rs",
        "#[allow(dead_code)]\npub mod inner;",
        "src/inner.rs",
        "pub fn unused() {}",
    );
    assert!(dead_code_warnings(&on_the_declaration).is_empty());
    let reported = parse2(
        "src/lib.rs",
        "pub mod inner;",
        "src/inner.rs",
        "pub fn unused() {}",
    );
    assert_eq!(
        dead_code_warnings(&reported).len(),
        1,
        "without the allow it is still dead"
    );
}

#[test]
fn a_function_handed_to_a_call_through_macro_is_called() {
    // `$test:path` in callee position: the invocation passes a bare name, and
    // nothing in the token walk sees it as a call — an ident followed by a
    // comma is not in call position. The functions the suite really runs were
    // reported as never called, which is how a codebase ends up papering over
    // it with `qual:api` and hiding the genuinely dead code underneath.
    let code = r#"
macro_rules! run_step {
    ($test:path, $make:path) => {
        $test(&$make());
    };
}
fn make_store() -> S { S }
fn check_append(s: &S) {}
fn suite() { run_step!(check_append, make_store); }
fn main() { suite(); }
"#;
    let found = dead_code_warnings(&parse(code));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_call_through_reaches_a_macro_that_only_forwards() {
    // The real shape has two levels: the entry macro forwards its metavariable
    // to the one that does the calling. Only following the chain gets the
    // invocation site right.
    let code = r#"
macro_rules! step {
    ($test:path, $make:path) => {
        $test(&$make());
    };
}
macro_rules! run_suite {
    ($make:path; $($test:path),*) => {
        $( step!($test, $make); )*
    };
}
fn make_store() -> S { S }
fn check_append(s: &S) {}
fn suite() { run_suite!(make_store; check_append); }
fn main() { suite(); }
"#;
    let found = dead_code_warnings(&parse(code));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_ordinary_macro_invocation_does_not_vouch_for_its_arguments() {
    // The bound the fix needs: only a macro that really calls through a
    // metavariable licenses harvesting every ident at its invocation. Without
    // that trigger, `assert_eq!(a, dead_helper)` would mark a plainly dead
    // function as called — which is the mistake that costs a real finding.
    let code = r#"
fn dead_helper() -> u8 { 1 }
fn used() { let x = 1u8; assert_eq!(x, 1); }
fn main() { used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].function_name, "dead_helper");
}

#[test]
fn a_macro_that_only_stringifies_does_not_vouch_for_its_argument() {
    // `stringify!($f())` turns the tokens into text and calls nothing. Reading
    // every `$name(` in a macro body as a call classified this as
    // call-through, and the invocation then excused a plainly dead function
    // from DRY-002 and TQ-003 — a masked finding, which is the whole thing
    // these checks exist to prevent.
    let code = r#"
macro_rules! name_of {
    ($f:path) => { stringify!($f()) };
}
fn dead_helper() -> u8 { 1 }
fn used() -> &'static str { name_of!(dead_helper) }
fn main() { let _ = used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].function_name, "dead_helper");
}

#[test]
fn a_macro_that_formats_the_result_still_calls_through() {
    // The counterpart: `println!("{}", $f())` really does run `$f`. Excluding
    // every nested macro would have re-opened the bug this fix closed, so only
    // the token-to-text macros are excluded.
    let code = r#"
macro_rules! report {
    ($f:path) => { println!("{}", $f()); };
}
fn live_helper() -> u8 { 1 }
fn used() { report!(live_helper); }
fn main() { used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn merely_naming_a_call_through_macro_is_not_forwarding() {
    // Forwarding means handing a metavariable to the macro that calls it. A
    // body that only mentions the name — here inside `stringify!` — passes
    // nothing on, and treating it as a forwarder let its invocation excuse a
    // function nothing runs.
    let code = r#"
macro_rules! step {
    ($t:path) => { $t(); };
}
macro_rules! mention {
    ($x:path) => { stringify!(step) };
}
fn dead_helper() {}
fn used() -> &'static str { mention!(dead_helper) }
fn main() { let _ = used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].function_name, "dead_helper");
}

#[test]
fn quote_spanned_does_not_execute_either() {
    // `quote!` was excluded and `quote_spanned!` was not, though both only turn
    // their input into tokens. A list of names has to be complete to be worth
    // anything.
    let code = r#"
macro_rules! emit {
    ($f:path) => { quote_spanned!(sp => $f()) };
}
fn dead_helper() {}
fn used() { emit!(dead_helper); }
fn main() { used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].function_name, "dead_helper");
}

#[test]
fn a_metavariable_that_is_only_stringified_is_not_forwarded() {
    // The target calls its *first* argument; the invocation hands it a fixed
    // name and only stringifies the metavariable. Reading any `$` in the
    // invocation as a forward let `wrapper!(dead_helper)` excuse a function
    // that is never called.
    let code = r#"
macro_rules! step {
    ($f:path, $label:expr) => { $f(); };
}
macro_rules! wrapper {
    ($x:path) => { step!(live_helper, stringify!($x)); };
}
fn live_helper() {}
fn dead_helper() {}
fn used() { wrapper!(dead_helper); }
fn main() { used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    let names: Vec<&str> = found.iter().map(|w| w.function_name.as_str()).collect();
    assert!(names.contains(&"dead_helper"), "{found:?}");
    // `live_helper` is reported too, and that is a different, older gap: it is
    // named in the macro *definition* in argument position, and only macro
    // invocations feed the production call graph. Asserting the total here
    // would pin that unrelated behaviour into this test.
}

#[test]
fn a_metavariable_at_a_position_the_target_never_calls_is_not_forwarded() {
    // `step!` applies its *first* argument. The wrapper passes a fixed name
    // there and only consumes `$x`, so nothing it is invoked with is ever
    // called — reading any metavariable in the argument list as a forward
    // excused a plainly dead function.
    let code = r#"
macro_rules! step {
    ($f:path, $label:expr) => { $f(); };
}
macro_rules! wrapper {
    ($x:path) => { step!(live_helper, consume($x)); };
}
fn live_helper() {}
fn consume(_: u8) {}
fn dead_helper() {}
fn used() { wrapper!(dead_helper); }
fn main() { used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    let names: Vec<&str> = found.iter().map(|w| w.function_name.as_str()).collect();
    assert!(names.contains(&"dead_helper"), "{found:?}");
}

#[test]
fn a_metavariable_at_the_called_position_is_forwarded() {
    // The counterpart: same target, and now the wrapper really does hand its
    // own metavariable to the argument `step!` applies.
    let code = r#"
macro_rules! step {
    ($f:path, $label:expr) => { $f(); };
}
macro_rules! wrapper {
    ($x:path) => { step!($x, consume(1)); };
}
fn consume(_: u8) {}
fn live_helper() {}
fn used() { wrapper!(live_helper); }
fn main() { used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_forwarder_keeps_its_called_position_for_the_next_hop() {
    // Three levels: `step!` applies argument 0, `middle!` hands its own
    // argument 0 there, and `outer!` passes a fixed name to that position while
    // only consuming its metavariable. Storing "unknown" for every forwarder
    // threw away what had just been computed, so the second hop accepted any
    // metavariable again and excused a dead function.
    let code = r#"
macro_rules! step {
    ($f:path, $label:expr) => { $f(); };
}
macro_rules! middle {
    ($g:path, $note:expr) => { step!($g, $note); };
}
macro_rules! outer {
    ($x:path) => { middle!(live_helper, consume($x)); };
}
fn live_helper() {}
fn consume(_: u8) {}
fn dead_helper() {}
fn used() { outer!(dead_helper); }
fn main() { used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    let names: Vec<&str> = found.iter().map(|w| w.function_name.as_str()).collect();
    assert!(names.contains(&"dead_helper"), "{found:?}");
}

#[test]
fn a_chain_that_really_forwards_still_reaches_the_callee() {
    // The counterpart over the same three levels: each hop hands its
    // metavariable to the position the next one calls.
    let code = r#"
macro_rules! step {
    ($f:path, $label:expr) => { $f(); };
}
macro_rules! middle {
    ($g:path, $note:expr) => { step!($g, $note); };
}
macro_rules! outer {
    ($x:path) => { middle!($x, consume(1)); };
}
fn consume(_: u8) {}
fn live_helper() {}
fn used() { outer!(live_helper); }
fn main() { used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    assert!(found.is_empty(), "{found:?}");
}

/// Which names an invocation really calls, decided by the arm `macro_rules!`
/// itself would take: `(label, code, reported_dead, treated_as_called)`.
type PositionCase = (&'static str, &'static str, &'static [&'static str]);

const CALLED_POSITION_CASES: &[(PositionCase, &[&str])] = &[
    (
        (
            "one rule applies argument 0",
            r#"
macro_rules! step {
    ($f:path, $label:path) => { $f(); };
}
fn live_helper() {}
fn dead_helper() {}
fn used() { step!(live_helper, dead_helper); }
fn main() { used(); }
"#,
            &["dead_helper"],
        ),
        &["live_helper"],
    ),
    (
        (
            // Two rules, mirrored. `macro_rules!` takes the first that matches,
            // so unioning the positions across rules said both arguments were
            // called and the second name lost its finding.
            "the first matching rule",
            r#"
macro_rules! choose {
    ($f:path, $value:expr) => { $f(); };
    ($value:expr, $f:path) => { $f(); };
}
fn live_helper() {}
fn dead_helper() {}
fn used() { choose!(live_helper, dead_helper); }
fn main() { used(); }
"#,
            &["dead_helper"],
        ),
        &["live_helper"],
    ),
    (
        (
            // The first arm matches and applies nothing; the later arm applies
            // its first parameter. Dropping the silent arms before selection
            // let that one answer for an invocation it never sees.
            "a silent first arm, a later applying one",
            r#"
macro_rules! choose {
    ($value:expr, $label:path) => { consume($value); };
    ($f:path, $value:expr) => { $f(); };
}
fn consume(_: u8) {}
fn live_helper() {}
fn dead_helper() {}
fn used() { choose!(live_helper, dead_helper); }
fn main() { used(); }
"#,
            &["live_helper", "dead_helper"],
        ),
        &[],
    ),
    (
        (
            // Same, with the later arm unreadable instead. That says nothing
            // about an invocation the first arm already matches; collapsing the
            // list on it fell back to "every argument is a call".
            "a silent first arm, a later unreadable one",
            r#"
macro_rules! choose {
    ($value:expr, $label:path) => { consume($value); };
    ($($f:path),*) => { $( $f(); )* };
}
fn consume(_: u8) {}
fn live_helper() {}
fn dead_helper() {}
fn used() { choose!(live_helper, dead_helper); }
fn main() { used(); }
"#,
            &["live_helper", "dead_helper"],
        ),
        &[],
    ),
    (
        (
            // `$body:block` does not accept a bare path, so rustc takes the
            // second arm and calls the function. Treating an unchecked fragment
            // as a match picked the first arm, registered no call, and reported
            // a live function as dead — the expensive direction.
            "a fragment that rejects the argument",
            r#"
macro_rules! choose {
    ($body:block) => { $body };
    ($f:path) => { $f(); };
}
fn live_helper() {}
fn used() { choose!(live_helper); }
fn main() { used(); }
"#,
            &[],
        ),
        &["live_helper"],
    ),
    (
        (
            // `vis` is the one fragment this cannot decide — it matches the
            // empty token stream too. Undecidable has to mean undecidable: the
            // walk stops and every argument counts.
            "a fragment that cannot be decided",
            r#"
macro_rules! choose {
    ($v:vis) => { };
    ($f:path) => { $f(); };
}
fn live_helper() {}
fn used() { choose!(live_helper); }
fn main() { used(); }
"#,
            &[],
        ),
        &["live_helper"],
    ),
    (
        (
            // Forwarded: `$f` was captured as a `path`, and rustc keeps that
            // when it is handed on — the `block` arm does not take it. Reading
            // a metavariable as "accepts anything" picked that arm, which
            // applies nothing, and reported the function the real arm calls as
            // dead.
            "a metavariable keeps its fragment when forwarded",
            r#"
macro_rules! choose {
    ($body:block) => { $body };
    ($f:path) => { $f(); };
}
macro_rules! wrapper {
    ($f:path) => { choose!($f); };
}
fn live_helper() {}
fn used() { wrapper!(live_helper); }
fn main() { used(); }
"#,
            &[],
        ),
        &["live_helper"],
    ),
    (
        (
            // The same one step subtler: substituting the metavariable by a
            // plain identifier let an `ident` arm take a forwarded `path`.
            // rustc keeps a matched fragment opaque — only a matcher of the
            // same kind consumes it.
            "a forwarded path is not an ident",
            r#"
macro_rules! choose {
    ($ignored:ident) => {};
    ($f:path) => { $f(); };
}
macro_rules! wrapper {
    ($f:path) => { choose!($f); };
}
fn live_helper() {}
fn used() { wrapper!(live_helper); }
fn main() { used(); }
"#,
            &[],
        ),
        &["live_helper"],
    ),
    (
        (
            // `expr_2021` and `expr` are one fragment under two names — rustc
            // takes the first arm and calls the function. Comparing the names
            // alone rejected it, picked the empty arm, and could report a live
            // function as dead.
            "an edition variant of the same fragment",
            r#"
macro_rules! choose {
    ($f:expr) => { $f(); };
    ($ignored:expr_2021) => {};
}
macro_rules! wrapper {
    ($f:expr_2021) => { choose!($f); };
}
fn live_helper() {}
fn used() { wrapper!(live_helper); }
fn main() { used(); }
"#,
            &[],
        ),
        &["live_helper"],
    ),
    (
        (
            // `const { … }` is an `expr` from edition 2024 and never an
            // `expr_2021`. Parsing both with the same parser let the first arm
            // take it, and that arm applies nothing.
            "an edition-2024 expression is not an expr_2021",
            r#"
macro_rules! choose {
    ($ignored:expr_2021, $also:path) => {};
    ($modern:expr, $f:path) => { $f(); };
}
fn live_helper() {}
fn used() { choose!(const { 1 }, live_helper); }
fn main() { used(); }
"#,
            &[],
        ),
        &["live_helper"],
    ),
];

#[test]
fn only_the_called_argument_positions_count_as_calls() {
    for ((label, code, dead), called) in CALLED_POSITION_CASES {
        let found = dead_code_warnings(&parse(code));
        let names: Vec<&str> = found.iter().map(|w| w.function_name.as_str()).collect();
        dead.iter()
            .for_each(|name| assert!(names.contains(name), "{label}: {name} {found:?}"));
        called
            .iter()
            .for_each(|name| assert!(!names.contains(name), "{label}: {name} {found:?}"));
    }
}

#[test]
fn a_macro_that_applies_nothing_is_not_call_through() {
    // A matcher no position model fits does not make a macro call-through: it
    // has to apply a metavariable somewhere. Otherwise every macro with a
    // repetition would harvest its whole invocation.
    let code = r#"
macro_rules! report {
    ($($x:expr),*) => { let _ = ($($x),*); };
}
fn dead_helper() {}
fn used() { report!(1, dead_helper); }
fn main() { used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    let names: Vec<&str> = found.iter().map(|w| w.function_name.as_str()).collect();
    assert!(names.contains(&"dead_helper"), "{found:?}");
}

#[test]
fn an_earlier_unreadable_arm_does_trigger_the_fallback() {
    // The other order: nothing can say whether the repetition arm matches
    // first, so the coarse rule has to hold — a suite runner really does call
    // all of them, and inventing "never called" there is the expensive mistake.
    let code = r#"
macro_rules! choose {
    ($($f:path),*) => { $( $f(); )* };
    ($value:expr, $label:path) => { consume($value); };
}
fn consume(_: u8) {}
fn live_helper() {}
fn dead_helper() {}
fn used() { choose!(live_helper, dead_helper); }
fn main() { used(); }
"#;
    let found = dead_code_warnings(&parse(code));
    assert!(found.is_empty(), "{found:?}");
}
