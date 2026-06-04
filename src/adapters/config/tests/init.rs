use crate::adapters::config::init::*;
use crate::config::Config;

/// Build a `ProjectMetrics` carrying just the complexity maxima (file/function
/// counts fixed at 1 — they only affect the generated comment text, not the
/// thresholds). Keeps repeated constructions out of `BP-009`'s window.
fn complexity_metrics(
    max_cognitive: usize,
    max_cyclomatic: usize,
    max_nesting_depth: usize,
    max_function_lines: usize,
) -> ProjectMetrics {
    ProjectMetrics {
        file_count: 1,
        function_count: 1,
        max_cognitive,
        max_cyclomatic,
        max_nesting_depth,
        max_function_lines,
    }
}

/// Generate a tailored config for `metrics` and parse it back into a `Config`.
fn tailored_config(metrics: &ProjectMetrics) -> Config {
    toml::from_str(&generate_tailored_config(metrics)).expect("tailored config must be valid TOML")
}

#[test]
fn test_generate_default_config_is_valid_toml() {
    let content = generate_default_config();
    let result: Result<Config, _> = toml::from_str(content);
    assert!(
        result.is_ok(),
        "Generated config must be valid TOML: {:?}",
        result.err()
    );
}

#[test]
fn test_generate_default_config_contents() {
    let content = generate_default_config();
    assert!(content.contains("ignore_functions"));
    assert!(content.contains("strict_closures"));
    assert!(content.contains("allow_recursion"));
    assert!(content.contains("strict_error_propagation"));
    assert!(content.contains("[complexity]"));
    assert!(content.contains("[duplicates]"));
    assert!(content.contains("[boilerplate]"));
    assert!(content.contains("[srp]"));
    assert!(content.contains("[coupling]"));
    assert!(content.contains("[tests]"));
    assert!(content.contains("max_suppression_ratio"));
    assert!(content.contains("fail_on_warnings"));
    assert!(content.contains("[weights]"));
    assert!(content.contains("iosp         = 0.22"));
    assert!(content.contains("architecture = 0.10"));
    assert!(content.contains("test_quality = 0.10"));
}

#[test]
fn test_generate_tailored_config_is_valid_toml() {
    let content = generate_tailored_config(&complexity_metrics(12, 8, 3, 45));
    let result: Result<Config, _> = toml::from_str(&content);
    assert!(
        result.is_ok(),
        "Tailored config must be valid TOML: {:?}",
        result.err()
    );
}

#[test]
fn tailored_config_complexity_thresholds() {
    // Thresholds are set to 1.2× the observed maxima, but never below the
    // built-in defaults (so a tiny project clamps to the defaults).
    // (label, metrics, expected [cognitive, cyclomatic, nesting, function_lines])
    use crate::adapters::config::sections::{
        DEFAULT_MAX_COGNITIVE, DEFAULT_MAX_CYCLOMATIC, DEFAULT_MAX_FUNCTION_LINES,
        DEFAULT_MAX_NESTING_DEPTH,
    };
    let cases: &[(&str, ProjectMetrics, [usize; 4])] = &[
        (
            "1.2× headroom above observed maxima",
            complexity_metrics(20, 15, 5, 80),
            [24, 18, 6, 96],
        ),
        (
            "tiny project clamps to the built-in defaults",
            complexity_metrics(3, 2, 1, 10),
            [
                DEFAULT_MAX_COGNITIVE,
                DEFAULT_MAX_CYCLOMATIC,
                DEFAULT_MAX_NESTING_DEPTH,
                DEFAULT_MAX_FUNCTION_LINES,
            ],
        ),
    ];
    for (label, metrics, [cognitive, cyclomatic, nesting, function_lines]) in cases {
        let c = tailored_config(metrics).complexity;
        assert_eq!(c.max_cognitive, *cognitive, "case {label}: cognitive");
        assert_eq!(c.max_cyclomatic, *cyclomatic, "case {label}: cyclomatic");
        assert_eq!(c.max_nesting_depth, *nesting, "case {label}: nesting");
        assert_eq!(
            c.max_function_lines, *function_lines,
            "case {label}: function_lines"
        );
    }
}

#[test]
fn test_generate_tailored_config_includes_metrics_comments() {
    let metrics = ProjectMetrics {
        file_count: 42,
        function_count: 100,
        max_cognitive: 10,
        max_cyclomatic: 8,
        max_nesting_depth: 3,
        max_function_lines: 50,
    };
    let content = generate_tailored_config(&metrics);
    assert!(content.contains("42 file(s)"));
    assert!(content.contains("100 function(s)"));
    assert!(content.contains("current max: 10"));
    assert!(content.contains("current max: 8"));
    assert!(content.contains("[tests]"));
}
