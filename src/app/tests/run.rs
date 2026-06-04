use crate::app::setup::apply_cli_overrides;
use crate::cli::{Cli, OutputFormat};
use crate::config::{self, Config};
use crate::determine_output_format;
use clap::Parser;

// ── OutputFormat tests ──────────────────────────────────────

#[test]
fn test_output_format_text() {
    assert_eq!("text".parse::<OutputFormat>().unwrap(), OutputFormat::Text);
}

#[test]
fn test_output_format_json() {
    assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
}

#[test]
fn test_output_format_github() {
    assert_eq!(
        "github".parse::<OutputFormat>().unwrap(),
        OutputFormat::Github
    );
}

#[test]
fn test_output_format_dot() {
    assert_eq!("dot".parse::<OutputFormat>().unwrap(), OutputFormat::Dot);
}

#[test]
fn test_output_format_sarif() {
    assert_eq!(
        "sarif".parse::<OutputFormat>().unwrap(),
        OutputFormat::Sarif
    );
}

#[test]
fn test_output_format_html() {
    assert_eq!("html".parse::<OutputFormat>().unwrap(), OutputFormat::Html);
}

#[test]
fn test_output_format_ai() {
    assert_eq!("ai".parse::<OutputFormat>().unwrap(), OutputFormat::Ai);
}

#[test]
fn test_output_format_ai_json() {
    assert_eq!(
        "ai-json".parse::<OutputFormat>().unwrap(),
        OutputFormat::AiJson
    );
}

#[test]
fn test_output_format_case_insensitive() {
    assert_eq!("JSON".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
    assert_eq!("Text".parse::<OutputFormat>().unwrap(), OutputFormat::Text);
    assert_eq!(
        "GITHUB".parse::<OutputFormat>().unwrap(),
        OutputFormat::Github
    );
}

#[test]
fn test_output_format_invalid() {
    assert!("xml".parse::<OutputFormat>().is_err());
    assert!("csv".parse::<OutputFormat>().is_err());
}

#[test]
fn test_output_format_default() {
    assert_eq!(OutputFormat::default(), OutputFormat::Text);
}

// ── CLI override tests ──────────────────────────────────────

#[test]
fn test_apply_cli_overrides_strict_closures() {
    let mut config = Config::default();
    let cli = Cli::parse_from(["test", "--strict-closures"]);
    apply_cli_overrides(&mut config, &cli);
    assert!(config.strict_closures);
}

#[test]
fn test_apply_cli_overrides_allow_recursion() {
    let mut config = Config::default();
    let cli = Cli::parse_from(["test", "--allow-recursion"]);
    apply_cli_overrides(&mut config, &cli);
    assert!(config.allow_recursion);
}

#[test]
fn test_apply_cli_overrides_strict_error_propagation() {
    let mut config = Config::default();
    let cli = Cli::parse_from(["test", "--strict-error-propagation"]);
    apply_cli_overrides(&mut config, &cli);
    assert!(config.strict_error_propagation);
}

#[test]
fn test_apply_cli_overrides_strict_iterators() {
    let mut config = Config::default();
    let cli = Cli::parse_from(["test", "--strict-iterators"]);
    apply_cli_overrides(&mut config, &cli);
    assert!(config.strict_iterator_chains);
}

#[test]
fn test_apply_cli_overrides_no_flags() {
    let mut config = Config::default();
    let cli = Cli::parse_from(["test"]);
    apply_cli_overrides(&mut config, &cli);
    assert!(!config.strict_closures);
    assert!(!config.strict_iterator_chains);
    assert!(!config.allow_recursion);
    assert!(!config.strict_error_propagation);
}

#[test]
fn test_fail_on_warnings_cli_parse() {
    let cli = Cli::parse_from(["test", "--fail-on-warnings"]);
    assert!(cli.fail_on_warnings);
}

#[test]
fn test_fail_on_warnings_default_false() {
    let cli = Cli::parse_from(["test"]);
    assert!(!cli.fail_on_warnings);
}

#[test]
fn test_apply_cli_overrides_fail_on_warnings() {
    let mut config = Config::default();
    let cli = Cli::parse_from(["test", "--fail-on-warnings"]);
    apply_cli_overrides(&mut config, &cli);
    assert!(config.fail_on_warnings);
}

#[test]
fn test_fail_on_warnings_config_default() {
    let config = Config::default();
    assert!(!config.fail_on_warnings);
}

#[test]
fn test_determine_output_format_explicit() {
    let cli = Cli::parse_from(["test", "--format", "json"]);
    assert_eq!(determine_output_format(&cli), OutputFormat::Json);
}

#[test]
fn test_determine_output_format_json_flag() {
    let cli = Cli::parse_from(["test", "--json"]);
    assert_eq!(determine_output_format(&cli), OutputFormat::Json);
}

#[test]
fn test_determine_output_format_default_text() {
    let cli = Cli::parse_from(["test"]);
    assert_eq!(determine_output_format(&cli), OutputFormat::Text);
}

#[test]
fn test_determine_output_format_explicit_overrides_json_flag() {
    let cli = Cli::parse_from(["test", "--json", "--format", "html"]);
    assert_eq!(determine_output_format(&cli), OutputFormat::Html);
}

// ── Init metrics tests ────────────────────────────────────────

#[test]
fn test_extract_init_metrics_empty() {
    let m = config::init::extract_init_metrics(0, &[]);
    assert_eq!(m.file_count, 0);
    assert_eq!(m.function_count, 0);
    assert_eq!(m.max_cognitive, 0);
}

#[test]
fn test_extract_init_metrics_with_complexity() {
    let fa = crate::adapters::analyzers::iosp::FunctionAnalysis {
        name: "f".into(),
        file: "test.rs".into(),
        line: 1,
        classification: crate::adapters::analyzers::iosp::Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: Some(crate::adapters::analyzers::iosp::ComplexityMetrics {
            cognitive_complexity: 12,
            cyclomatic_complexity: 8,
            max_nesting: 3,
            function_lines: 45,
            ..Default::default()
        }),
        qualified_name: "f".into(),
        severity: None,
        cognitive_warning: false,
        cyclomatic_warning: false,
        nesting_depth_warning: false,
        function_length_warning: false,
        unsafe_warning: false,
        error_handling_warning: false,
        complexity_suppressed: false,
        own_calls: vec![],
        parameter_count: 0,
        is_trait_impl: false,
        is_test: false,
        effort_score: None,
    };
    let results = vec![fa];
    let m = config::init::extract_init_metrics(5, &results);
    assert_eq!(m.file_count, 5);
    assert_eq!(m.function_count, 1);
    assert_eq!(m.max_cognitive, 12);
    assert_eq!(m.max_cyclomatic, 8);
    assert_eq!(m.max_nesting_depth, 3);
    assert_eq!(m.max_function_lines, 45);
}
