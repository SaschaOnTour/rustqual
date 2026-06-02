use super::*;

// ── Basic call resolution ─────────────────────────────────────

#[test]
fn test_collect_direct_qualified_call() {
    let calls = calls_in(
        r#"
        pub fn cmd_search() {
            crate::application::stats::get_stats(1);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("crate::application::stats::get_stats"));
}

#[test]
fn test_collect_unqualified_via_use_alias() {
    let calls = calls_in(
        r#"
        use crate::application::stats::get_stats;
        pub fn cmd_search() {
            get_stats(1);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("crate::application::stats::get_stats"));
}

#[test]
fn test_collect_unqualified_no_alias_is_bare() {
    let calls = calls_in(
        r#"
        pub fn cmd_search() {
            foo(1);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("<bare>:foo"));
}

#[test]
fn test_collect_in_semicolon_separated_macro_descends() {
    // Regression: `vec![expr; n]`-style macros have tokens `expr ; n`
    // which fails the comma-list parser. The block fallback wraps them
    // in braces and extracts stmt-level expressions.
    let calls = calls_in(
        r#"
        pub fn cmd_search() {
            let _v = vec![compute(x); 3];
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(
        calls.contains("<bare>:compute"),
        "`;`-separated macro body must still descend, got {calls:?}"
    );
}

#[test]
fn test_collect_in_macro_descends() {
    let calls = calls_in(
        r#"
        pub fn cmd_search() {
            debug_assert!(validate(1));
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("<bare>:validate"));
    // The macro itself must not be recorded as a call target.
    assert!(!calls.contains("<macro>:debug_assert"));
}

#[test]
fn test_collect_self_super_prefix() {
    // `self::` inside `src/cli/mod.rs` resolves against module `crate::cli`,
    // so `self::helpers::format` → `crate::cli::helpers::format`. A non-mod
    // file (e.g. `src/cli/handlers.rs`) would resolve to
    // `crate::cli::handlers::helpers::format`, which is the Rust-semantic
    // outcome; the collector defers to `resolve_to_crate_absolute` for
    // this so both cases stay consistent with the file's module path.
    let calls = calls_in(
        r#"
        pub fn cmd_search() {
            self::helpers::format(1);
        }
        "#,
        "src/cli/mod.rs",
        "cmd_search",
    );
    assert!(
        calls.contains("crate::cli::helpers::format"),
        "calls = {:?}",
        calls
    );
}

#[test]
fn test_collect_turbofish_stripped() {
    let calls = calls_in(
        r#"
        pub fn cmd_search() {
            Box::<u32>::new(42);
        }
        "#,
        "src/cli/handlers.rs",
        "cmd_search",
    );
    assert!(calls.contains("<bare>:Box::new"));
}
