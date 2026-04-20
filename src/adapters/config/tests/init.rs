use crate::adapters::config::init::*;
use crate::config::Config;

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
    assert!(content.contains("max_suppression_ratio"));
    assert!(content.contains("fail_on_warnings"));
    assert!(content.contains("[weights]"));
    assert!(content.contains("iosp         = 0.22"));
    assert!(content.contains("architecture = 0.10"));
    assert!(content.contains("test_quality = 0.10"));
}

#[test]
fn test_generate_tailored_config_is_valid_toml() {
    let metrics = ProjectMetrics {
        file_count: 10,
        function_count: 50,
        max_cognitive: 12,
        max_cyclomatic: 8,
        max_nesting_depth: 3,
        max_function_lines: 45,
    };
    let content = generate_tailored_config(&metrics);
    let result: Result<Config, _> = toml::from_str(&content);
    assert!(
        result.is_ok(),
        "Tailored config must be valid TOML: {:?}",
        result.err()
    );
}

#[test]
fn test_generate_tailored_config_uses_headroom() {
    let metrics = ProjectMetrics {
        file_count: 5,
        function_count: 20,
        max_cognitive: 20,
        max_cyclomatic: 15,
        max_nesting_depth: 5,
        max_function_lines: 80,
    };
    let content = generate_tailored_config(&metrics);
    let cfg: Config = toml::from_str(&content).unwrap();
    assert_eq!(cfg.complexity.max_cognitive, 24);
    assert_eq!(cfg.complexity.max_cyclomatic, 18);
    assert_eq!(cfg.complexity.max_nesting_depth, 6);
    assert_eq!(cfg.complexity.max_function_lines, 96);
}

#[test]
fn test_generate_tailored_config_respects_minimums() {
    let metrics = ProjectMetrics {
        file_count: 1,
        function_count: 2,
        max_cognitive: 3,
        max_cyclomatic: 2,
        max_nesting_depth: 1,
        max_function_lines: 10,
    };
    let content = generate_tailored_config(&metrics);
    let cfg: Config = toml::from_str(&content).unwrap();
    assert_eq!(
        cfg.complexity.max_cognitive,
        crate::adapters::config::sections::DEFAULT_MAX_COGNITIVE
    );
    assert_eq!(
        cfg.complexity.max_cyclomatic,
        crate::adapters::config::sections::DEFAULT_MAX_CYCLOMATIC
    );
    assert_eq!(
        cfg.complexity.max_nesting_depth,
        crate::adapters::config::sections::DEFAULT_MAX_NESTING_DEPTH
    );
    assert_eq!(
        cfg.complexity.max_function_lines,
        crate::adapters::config::sections::DEFAULT_MAX_FUNCTION_LINES
    );
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
}
