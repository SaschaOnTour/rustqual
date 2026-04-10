mod analyzer;
mod cli;
mod config;
mod coupling;
mod dry;
mod findings;
mod normalize;
mod pipeline;
mod report;
mod scope;
mod srp;
mod structural;
mod tq;
mod watch;

use std::path::Path;

use clap::{CommandFactory, Parser};

use cli::{Cli, OutputFormat};
use config::Config;

/// Determine output format from CLI flags.
/// Operation: conditional logic.
fn determine_output_format(cli: &Cli) -> OutputFormat {
    if let Some(ref fmt) = cli.format {
        fmt.clone()
    } else if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    }
}

/// Handle the --init command: write a rustqual.toml config file.
/// Operation: logic to check file existence and write.
fn handle_init(content: &str) -> Result<(), i32> {
    let path = Path::new("rustqual.toml");
    if path.exists() {
        eprintln!("Error: rustqual.toml already exists in the current directory.");
        return Err(1);
    }
    match std::fs::write(path, content) {
        Ok(()) => {
            eprintln!("Created rustqual.toml with tailored configuration.");
            Ok(())
        }
        Err(e) => {
            eprintln!("Error writing rustqual.toml: {e}");
            Err(1)
        }
    }
}

/// Handle the --completions command: generate shell completions.
/// Integration: orchestrates clap_complete::generate with Cli::command.
fn handle_completions(shell: clap_complete::Shell) {
    clap_complete::generate(
        shell,
        &mut Cli::command(),
        "rustqual",
        &mut std::io::stdout(),
    );
}

/// Load config from an explicit config file path.
/// Operation: error handling logic.
fn load_explicit_config(config_path: &Path) -> Result<Config, i32> {
    match std::fs::read_to_string(config_path) {
        Ok(content) => match toml::from_str(&content) {
            Ok(c) => Ok(c),
            Err(e) => {
                eprintln!("Error parsing config: {e}");
                Err(2)
            }
        },
        Err(e) => {
            eprintln!("Error reading config: {e}");
            Err(2)
        }
    }
}

/// Load config via auto-discovery from the project path.
/// Operation: error mapping logic.
fn load_auto_config(path: &Path) -> Result<Config, i32> {
    Config::load(path).map_err(|e| {
        eprintln!("Error: {e}");
        2
    })
}

/// Load configuration from CLI args or auto-discovery.
/// Integration: delegates to load_explicit_config or load_auto_config.
fn load_config(cli: &Cli) -> Result<Config, i32> {
    cli.config
        .as_ref()
        .map(|p| load_explicit_config(p))
        .unwrap_or_else(|| load_auto_config(&cli.path))
}

/// Load, compile, and apply CLI overrides to config.
/// Integration: orchestrates load_config, compile, apply_cli_overrides, validate_weights.
fn setup_config(cli: &Cli) -> Result<Config, i32> {
    let mut config = load_config(cli)?;
    config.compile();
    apply_cli_overrides(&mut config, cli);
    validate_config_weights(&config)?;
    Ok(config)
}

/// Validate config settings that require cross-field checks.
/// Operation: error mapping logic.
fn validate_config_weights(config: &Config) -> Result<(), i32> {
    config::validate_weights(config).map_err(|e| {
        eprintln!("Error: {e}");
        2
    })
}

/// Apply CLI flag overrides to config.
/// Operation: conditional logic on CLI flags.
fn apply_cli_overrides(config: &mut Config, cli: &Cli) {
    if cli.strict_closures {
        config.strict_closures = true;
    }
    if cli.strict_iterators {
        config.strict_iterator_chains = true;
    }
    if cli.allow_recursion {
        config.allow_recursion = true;
    }
    if cli.strict_error_propagation {
        config.strict_error_propagation = true;
    }
    if cli.fail_on_warnings {
        config.fail_on_warnings = true;
    }
    if let Some(ref coverage) = cli.coverage {
        config.test.coverage_file = Some(coverage.display().to_string());
    }
}

/// Handle --save-baseline: write results to a JSON file.
/// Operation: serialization + file write logic.
fn handle_save_baseline(
    path: &Path,
    all_results: &[analyzer::FunctionAnalysis],
    summary: &report::Summary,
) -> Result<(), i32> {
    let baseline = report::create_baseline(all_results, summary);
    match std::fs::write(path, baseline) {
        Ok(()) => {
            eprintln!("Baseline saved to {}", path.display());
            Ok(())
        }
        Err(e) => {
            eprintln!("Error saving baseline: {e}");
            Err(1)
        }
    }
}

/// Handle --compare: compare current results against baseline.
/// Operation: file read + comparison logic.
fn handle_compare(
    path: &Path,
    all_results: &[analyzer::FunctionAnalysis],
    summary: &report::Summary,
) -> Result<bool, i32> {
    let baseline_content = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("Error reading baseline: {e}");
        1
    })?;
    Ok(report::print_comparison(
        &baseline_content,
        all_results,
        summary,
    ))
}

/// Check --min-quality-score gate.
/// Operation: conditional check.
fn check_min_quality_score(min_score: f64, summary: &report::Summary) -> Result<(), i32> {
    let actual = summary.quality_score * analyzer::PERCENTAGE_MULTIPLIER;
    if actual < min_score {
        eprintln!(
            "Quality score {:.1}% is below minimum {:.1}%",
            actual, min_score,
        );
        return Err(1);
    }
    Ok(())
}

/// Print a stderr warning if the suppression ratio exceeds the configured maximum.
/// Operation: conditional formatting logic.
fn warn_suppression_ratio(summary: &report::Summary, max_ratio: f64) {
    if !summary.suppression_ratio_exceeded || summary.total == 0 {
        return;
    }
    eprintln!(
        "Warning: {} suppression(s) found ({:.1}% of functions, max: {:.1}%)",
        summary.all_suppressions,
        summary.all_suppressions as f64 / summary.total as f64 * analyzer::PERCENTAGE_MULTIPLIER,
        max_ratio * analyzer::PERCENTAGE_MULTIPLIER,
    );
}

/// Check --fail-on-warnings gate.
/// Operation: conditional check.
fn check_fail_on_warnings(config: &Config, summary: &report::Summary) -> Result<(), i32> {
    if config.fail_on_warnings && summary.suppression_ratio_exceeded {
        eprintln!("Error: warnings present and --fail-on-warnings is set");
        return Err(1);
    }
    Ok(())
}

/// Apply quality gate checks from CLI flags.
/// Integration: dispatches to check_min_quality_score.
fn check_quality_gates(cli: &Cli, summary: &report::Summary) -> Result<(), i32> {
    cli.min_quality_score
        .iter()
        .try_for_each(|&s| check_min_quality_score(s, summary))
}

/// Check default-fail gate: exit 1 on findings unless --no-fail.
/// Operation: conditional check.
fn check_default_fail(no_fail: bool, total_findings: usize) -> Result<(), i32> {
    if !no_fail && total_findings > 0 {
        return Err(1);
    }
    Ok(())
}

/// Apply all exit gates: warnings, fail-on-warnings, quality gates, default-fail.
/// Integration: dispatches to warning + gate check functions.
fn apply_exit_gates(cli: &Cli, config: &Config, summary: &report::Summary) -> Result<(), i32> {
    warn_suppression_ratio(summary, config.max_suppression_ratio);
    check_fail_on_warnings(config, summary)?;
    check_quality_gates(cli, summary)?;
    check_default_fail(cli.no_fail, summary.total_findings())
}

/// Sort results so violations come first, ordered by effort score (highest first).
/// Operation: sorting logic.
fn sort_by_effort(results: &mut [analyzer::FunctionAnalysis]) {
    results.sort_by(|a, b| {
        b.effort_score
            .unwrap_or(0.0)
            .partial_cmp(&a.effort_score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Entry point: parse CLI, load config, run analysis, check gates.
pub fn run() -> Result<(), i32> {
    let mut args: Vec<String> = std::env::args().collect();
    // Support `cargo qual` invocation: cargo passes "qual" as first arg
    if args.len() > 1 && args[1] == "qual" {
        args.remove(1);
    }
    let mut cli = Cli::parse_from(args);
    // Normalize Windows backslash paths to forward slashes
    let normalized = cli.path.to_string_lossy().replace('\\', "/");
    cli.path = std::path::PathBuf::from(normalized);

    if cli.init {
        let files = pipeline::collect_rust_files(&cli.path);
        let content = if files.is_empty() {
            config::generate_default_config().to_string()
        } else {
            let parsed = pipeline::read_and_parse_files(&files, &cli.path);
            let default_config = Config::default();
            let scope_refs: Vec<(&str, &syn::File)> =
                parsed.iter().map(|(p, _, f)| (p.as_str(), f)).collect();
            let scope = scope::ProjectScope::from_files(&scope_refs);
            let analyzer_obj = analyzer::Analyzer::new(&default_config, &scope);
            let all_results: Vec<_> = parsed
                .iter()
                .flat_map(|(path, _, syntax)| analyzer_obj.analyze_file(syntax, path))
                .collect();
            let metrics = config::init::extract_init_metrics(files.len(), &all_results);
            config::generate_tailored_config(&metrics)
        };
        return handle_init(&content);
    }
    if let Some(shell) = cli.completions {
        handle_completions(shell);
        return Ok(());
    }

    let output_format = determine_output_format(&cli);
    let config = setup_config(&cli)?;

    if cli.watch {
        return watch::run_watch_mode(&cli, &config, &output_format);
    }

    let files = pipeline::collect_filtered_files(&cli.path, &config);
    let files = if let Some(ref git_ref) = cli.diff {
        match pipeline::get_git_changed_files(&cli.path, git_ref) {
            Ok(changed) => {
                let filtered = pipeline::filter_to_changed(files, &changed);
                eprintln!(
                    "[diff mode: {} changed file(s) vs {git_ref}]",
                    filtered.len()
                );
                filtered
            }
            Err(e) => {
                eprintln!("Warning: {e}. Analyzing all files.");
                files
            }
        }
    } else {
        files
    };
    if files.is_empty() {
        eprintln!("No Rust source files found in {}", cli.path.display());
        return Ok(());
    }

    let parsed = pipeline::read_and_parse_files(&files, &cli.path);
    let mut analysis = pipeline::run_analysis(&parsed, &config);
    if cli.sort_by_effort {
        sort_by_effort(&mut analysis.results);
    }
    if cli.findings {
        let entries = crate::report::findings_list::collect_all_findings(&analysis);
        if entries.is_empty() {
            println!("No findings.");
        } else {
            crate::report::findings_list::print_findings(&entries);
        }
    } else {
        pipeline::output_results(
            &analysis,
            &output_format,
            cli.verbose,
            cli.suggestions,
            &config,
        );
    }

    cli.save_baseline
        .as_ref()
        .map(|p| handle_save_baseline(p, &analysis.results, &analysis.summary))
        .transpose()?;
    if let Some(ref compare_path) = cli.compare {
        let regressed = handle_compare(compare_path, &analysis.results, &analysis.summary)?;
        if cli.fail_on_regression && regressed {
            return Err(1);
        }
    }

    apply_exit_gates(&cli, &config, &analysis.summary)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── Gate function tests (Result-based) ─────────────────────

    #[test]
    fn test_check_fail_on_warnings_passes_when_no_warnings() {
        let mut config = Config::default();
        config.fail_on_warnings = true;
        let summary = crate::report::Summary {
            total: 10,
            ..Default::default()
        };
        assert!(check_fail_on_warnings(&config, &summary).is_ok());
    }

    #[test]
    fn test_check_fail_on_warnings_passes_when_disabled() {
        let config = Config::default(); // fail_on_warnings = false
        let summary = crate::report::Summary {
            total: 10,
            suppression_ratio_exceeded: true,
            ..Default::default()
        };
        assert!(check_fail_on_warnings(&config, &summary).is_ok());
    }

    #[test]
    fn test_check_fail_on_warnings_exits_when_triggered() {
        let mut config = Config::default();
        config.fail_on_warnings = true;
        let summary = crate::report::Summary {
            total: 10,
            suppression_ratio_exceeded: true,
            ..Default::default()
        };
        assert_eq!(check_fail_on_warnings(&config, &summary), Err(1));
    }

    #[test]
    fn test_min_quality_score_cli_parse() {
        let cli = Cli::parse_from(["test", "--min-quality-score", "80.0"]);
        assert!((cli.min_quality_score.unwrap() - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_check_quality_gates_passes() {
        let cli = Cli::parse_from(["test", "--min-quality-score", "50.0"]);
        let mut summary = crate::report::Summary {
            total: 10,
            iosp_score: 1.0,
            ..Default::default()
        };
        summary.compute_quality_score(&crate::config::sections::DEFAULT_QUALITY_WEIGHTS);
        assert!(check_quality_gates(&cli, &summary).is_ok());
    }

    #[test]
    fn test_check_min_quality_score_below_threshold() {
        let mut summary = crate::report::Summary {
            total: 10,
            ..Default::default()
        };
        summary.quality_score = 0.5;
        assert_eq!(check_min_quality_score(90.0, &summary), Err(1));
    }

    #[test]
    fn test_check_min_quality_score_above_threshold() {
        let mut summary = crate::report::Summary {
            total: 10,
            ..Default::default()
        };
        summary.quality_score = 0.95;
        assert!(check_min_quality_score(90.0, &summary).is_ok());
    }

    #[test]
    fn test_check_quality_gates_below_threshold() {
        let cli = Cli::parse_from(["test", "--min-quality-score", "90.0"]);
        let summary = crate::report::Summary {
            total: 10,
            quality_score: 0.5,
            ..Default::default()
        };
        assert_eq!(check_quality_gates(&cli, &summary), Err(1));
    }

    #[test]
    fn test_check_quality_gates_no_gate_set() {
        let cli = Cli::parse_from(["test"]);
        let summary = crate::report::Summary {
            total: 10,
            ..Default::default()
        };
        assert!(check_quality_gates(&cli, &summary).is_ok());
    }

    #[test]
    fn test_check_default_fail_with_findings() {
        assert_eq!(check_default_fail(false, 5), Err(1));
    }

    #[test]
    fn test_check_default_fail_no_fail_mode() {
        assert!(check_default_fail(true, 5).is_ok());
    }

    #[test]
    fn test_check_default_fail_no_findings() {
        assert!(check_default_fail(false, 0).is_ok());
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
        let fa = crate::analyzer::FunctionAnalysis {
            name: "f".into(),
            file: "test.rs".into(),
            line: 1,
            classification: crate::analyzer::Classification::Operation,
            parent_type: None,
            suppressed: false,
            complexity: Some(crate::analyzer::ComplexityMetrics {
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

    // ── Diff CLI flag tests ──────────────────────────────────────

    #[test]
    fn test_diff_cli_default_ref() {
        let cli = Cli::parse_from(["test", "--diff"]);
        assert_eq!(cli.diff.as_deref(), Some("HEAD"));
    }

    #[test]
    fn test_diff_cli_custom_ref() {
        let cli = Cli::parse_from(["test", "--diff", "main"]);
        assert_eq!(cli.diff.as_deref(), Some("main"));
    }

    #[test]
    fn test_diff_cli_not_set() {
        let cli = Cli::parse_from(["test"]);
        assert!(cli.diff.is_none());
    }
}
