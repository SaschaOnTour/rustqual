use super::*;

#[test]
fn count_rust_allow_attrs_excludes_test_module_attrs() {
    // Only production `#[allow(...)]` attrs count toward the suppression
    // ratio — attrs that belong to a `#[cfg(test)] mod tests` attribute
    // group (contiguous, no blank-line gap) or live inside it are excluded.
    // (label, source, expected)
    let cases: &[(&str, &str, usize)] = &[
        (
            // #[allow] directly before #[cfg(test)] belongs to the test module
            "allow directly before cfg(test)",
            "#[allow(dead_code)]\n#[cfg(test)]\nmod tests {}",
            0,
        ),
        (
            // a non-attribute line breaks the group → production code
            "allow with a gap before cfg(test)",
            "#[allow(dead_code)]\nfn foo() {}\n#[cfg(test)]\nmod tests {}",
            1,
        ),
        (
            // #[derive] + #[allow] both part of the test-module attr group
            "derive and allow before cfg(test)",
            "#[derive(Debug)]\n#[allow(dead_code)]\n#[cfg(test)]\nmod tests {}",
            0,
        ),
        (
            "no cfg(test) counts all",
            "#[allow(dead_code)]\nfn foo() {}\n#[allow(unused)]\nfn bar() {}",
            2,
        ),
        (
            "cfg(test) on the first line",
            "#[cfg(test)]\nmod tests {\n#[allow(dead_code)]\nfn x() {}\n}",
            0,
        ),
        ("empty source", "", 0),
        (
            // blank-line gap separates a production allow from the test section
            "production allow before the test section",
            "#[allow(clippy::too_many_arguments)]\nfn big() {}\n\n#[cfg(test)]\nmod tests {}",
            1,
        ),
        (
            "allow inside the test module",
            "fn good() {}\n#[cfg(test)]\nmod tests {\n#[allow(dead_code)]\nfn helper() {}\n}",
            0,
        ),
    ];
    for (label, source, expected) in cases {
        assert_eq!(count_rust_allow_attrs(source), *expected, "case {label}");
    }
}

#[test]
fn extended_warning_flags_set_over_threshold() {
    // Each metric over its threshold sets the matching per-function warning
    // flag and increments the matching summary counter exactly once.
    // (label, metrics, flag, count)
    let cases: &[(&str, ComplexityMetrics, FlagFn, CountFn)] = &[
        (
            "nesting depth > 4",
            ComplexityMetrics {
                max_nesting: 5,
                ..Default::default()
            },
            (|f| f.nesting_depth_warning) as FlagFn,
            (|s| s.nesting_depth_warnings) as CountFn,
        ),
        (
            "function length > 60",
            ComplexityMetrics {
                function_lines: 61,
                ..Default::default()
            },
            |f| f.function_length_warning,
            |s| s.function_length_warnings,
        ),
        (
            "unsafe block present",
            ComplexityMetrics {
                unsafe_blocks: 1,
                ..Default::default()
            },
            |f| f.unsafe_warning,
            |s| s.unsafe_warnings,
        ),
        (
            "unwrap present",
            ComplexityMetrics {
                unwrap_count: 1,
                ..Default::default()
            },
            |f| f.error_handling_warning,
            |s| s.error_handling_warnings,
        ),
    ];
    for (label, metrics, flag, count) in cases {
        let (f, s) = apply_warnings(
            make_func_with_metrics(metrics.clone()),
            &Config::default(),
            &HashMap::new(),
        );
        assert!(flag(&f), "case {label}: flag should be set");
        assert_eq!(count(&s), 1, "case {label}: summary count");
    }
}

#[test]
fn test_nesting_depth_at_threshold_no_warning() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        max_nesting: 4,
        ..Default::default()
    })];
    apply_extended_warnings(
        &mut results,
        &config,
        &mut summary,
        &HashMap::new(),
        &HashMap::new(),
    );
    assert!(!results[0].nesting_depth_warning, "4 == threshold, no warn");
}

#[test]
fn test_function_length_at_threshold_no_warning() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        function_lines: 60,
        ..Default::default()
    })];
    apply_extended_warnings(
        &mut results,
        &config,
        &mut summary,
        &HashMap::new(),
        &HashMap::new(),
    );
    assert!(
        !results[0].function_length_warning,
        "60 == threshold, no warn"
    );
}

#[test]
fn test_error_handling_expect_allowed() {
    let mut config = Config::default();
    config.complexity.allow_expect = true;
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        expect_count: 3,
        ..Default::default()
    })];
    apply_extended_warnings(
        &mut results,
        &config,
        &mut summary,
        &HashMap::new(),
        &HashMap::new(),
    );
    assert!(
        !results[0].error_handling_warning,
        "expect allowed, no warn"
    );
}

#[test]
fn test_error_handling_expect_not_allowed() {
    let mut config = Config::default();
    config.complexity.allow_expect = false;
    let mut summary = Summary::default();
    let mut results = vec![make_func_with_metrics(ComplexityMetrics {
        expect_count: 1,
        ..Default::default()
    })];
    apply_extended_warnings(
        &mut results,
        &config,
        &mut summary,
        &HashMap::new(),
        &HashMap::new(),
    );
    assert!(
        results[0].error_handling_warning,
        "expect not allowed, should warn"
    );
}

#[test]
fn test_suppressed_functions_skipped() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut func = make_func_with_metrics(ComplexityMetrics {
        max_nesting: 10,
        function_lines: 100,
        unsafe_blocks: 3,
        unwrap_count: 5,
        ..Default::default()
    });
    func.suppressed = true;
    let mut results = vec![func];
    apply_extended_warnings(
        &mut results,
        &config,
        &mut summary,
        &HashMap::new(),
        &HashMap::new(),
    );
    assert!(!results[0].nesting_depth_warning);
    assert!(!results[0].function_length_warning);
    assert!(!results[0].unsafe_warning);
    assert!(!results[0].error_handling_warning);
}

#[test]
fn test_complexity_suppressed_functions_skipped() {
    let config = Config::default();
    let mut summary = Summary::default();
    let mut func = make_func_with_metrics(ComplexityMetrics {
        max_nesting: 10,
        function_lines: 100,
        ..Default::default()
    });
    func.complexity_suppressed = true;
    let mut results = vec![func];
    apply_extended_warnings(
        &mut results,
        &config,
        &mut summary,
        &HashMap::new(),
        &HashMap::new(),
    );
    assert!(!results[0].nesting_depth_warning);
    assert!(!results[0].function_length_warning);
}
