mod adapters;
mod app;
mod cli;
mod domain;
mod ports;
use adapters::config;
use adapters::report;
use adapters::source::watch;
use adapters::suppression::qual_allow as findings;
use cli::handlers::{handle_compare, handle_completions, handle_init, handle_save_baseline};

use clap::Parser;

use cli::{Cli, OutputFormat};

/// Determine output format from CLI flags.
/// Operation: conditional logic.
pub(crate) fn determine_output_format(cli: &Cli) -> OutputFormat {
    if let Some(ref fmt) = cli.format {
        fmt.clone()
    } else if cli.json {
        OutputFormat::Json
    } else {
        OutputFormat::Text
    }
}

use app::{apply_exit_gates, setup_config};

/// Sort results so violations come first, ordered by effort score (highest first).
/// Operation: sorting logic.
fn sort_by_effort(results: &mut [crate::adapters::analyzers::iosp::FunctionAnalysis]) {
    results.sort_by(|a, b| {
        b.effort_score
            .unwrap_or(0.0)
            .partial_cmp(&a.effort_score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Entry point: parse CLI, load config, run analysis, check gates.
/// Composition root: parse args, then run an early mode or the full analysis.
/// Integration: arg parse + mode dispatch (early-exit branches live in closures
/// so this stays pure orchestration).
pub fn run() -> Result<(), i32> {
    let cli = parse_cli_args();
    try_preconfig_modes(&cli).unwrap_or_else(|| run_configured(&cli))
}

/// Parse CLI args, supporting the `cargo qual` alias and normalising the path.
/// Integration: arg fixups + parse, delegating the logic to operations.
fn parse_cli_args() -> Cli {
    let args = strip_cargo_qual_arg(std::env::args().collect());
    let mut cli = Cli::parse_from(args);
    cli.path = normalize_path(&cli.path);
    cli
}

/// Drop the leading `qual` arg cargo injects for a `cargo qual` invocation.
/// Operation: conditional removal.
fn strip_cargo_qual_arg(mut args: Vec<String>) -> Vec<String> {
    if args.len() > 1 && args[1] == "qual" {
        args.remove(1);
    }
    args
}

/// Normalise Windows backslash paths to forward slashes.
/// Operation: string replace.
fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    std::path::PathBuf::from(path.to_string_lossy().replace('\\', "/"))
}

/// Early modes that need no loaded config (`--init`, `--completions`); returns
/// `Some(result)` when one handled the run. Operation: mode checks.
fn try_preconfig_modes(cli: &Cli) -> Option<Result<(), i32>> {
    if cli.init {
        let content = config::init::prepare_init_content(&cli.path);
        return Some(handle_init(&content));
    }
    if let Some(shell) = cli.completions {
        handle_completions(shell);
        return Some(Ok(()));
    }
    None
}

/// Load config, then dispatch a config-dependent mode or the full analysis.
/// Integration: config load + mode dispatch.
fn run_configured(cli: &Cli) -> Result<(), i32> {
    let config = setup_config(cli)?;
    try_postconfig_modes(cli, &config).unwrap_or_else(|| run_full_analysis(cli, &config))
}

/// Config-dependent early modes (`--explain`, `--watch`); `Some` when handled.
/// Operation: mode checks.
fn try_postconfig_modes(cli: &Cli, config: &config::Config) -> Option<Result<(), i32>> {
    if let Some(ref target) = cli.explain {
        // `--explain allow` prints the suppression guide instead of explaining
        // a file's architecture rules.
        if target.as_os_str() == "allow" {
            crate::cli::explain::explain_allow();
            return Some(Ok(()));
        }
        return Some(crate::cli::explain::handle_explain(target, config));
    }
    if cli.watch {
        let output_format = determine_output_format(cli);
        return Some(watch::run_watch_mode(&cli.path, || {
            app::analyze_and_output(
                &cli.path,
                config,
                &output_format,
                cli.verbose,
                cli.suggestions,
            );
        }));
    }
    None
}

/// The core pipeline: collect → parse → analyze → report → baseline → gates.
/// Integration: orchestrates the analysis phases.
fn run_full_analysis(cli: &Cli, config: &config::Config) -> Result<(), i32> {
    let files = adapters::source::filesystem::collect_filtered_files(&cli.path, config);
    let Some(analysis) = collect_and_analyze(cli, config, &files) else {
        return Ok(());
    };
    report_analysis(cli, &analysis, config);
    handle_baseline_and_compare(cli, &analysis)?;
    apply_exit_gates(cli, config, &analysis.summary)
}

/// Read + analyze `files`, applying effort sorting; `None` (with a message) when
/// there are no files to analyze. Operation: empty-guard + parse + analyze.
fn collect_and_analyze(
    cli: &Cli,
    config: &config::Config,
    files: &[std::path::PathBuf],
) -> Option<report::AnalysisResult> {
    if files.is_empty() {
        eprintln!("No Rust source files found in {}", cli.path.display());
        return None;
    }
    let parsed = adapters::source::filesystem::read_and_parse_files(files, &cli.path);
    let mut analysis = app::run_analysis(parsed, config);
    if cli.sort_by_effort {
        sort_by_effort(&mut analysis.results);
    }
    Some(analysis)
}

/// Render the analysis: either the flat findings list or the full report.
/// Operation: format selection.
fn report_analysis(cli: &Cli, analysis: &report::AnalysisResult, config: &config::Config) {
    let output_format = determine_output_format(cli);
    if cli.findings {
        let entries = crate::report::findings_list::collect_all_findings(analysis);
        if entries.is_empty() {
            println!("No findings.");
        } else {
            crate::report::findings_list::print_findings(&entries);
        }
    } else {
        app::output_results(
            analysis,
            &output_format,
            cli.verbose,
            cli.suggestions,
            config,
        );
    }
}

/// Apply `--save-baseline` and `--compare` (failing on regression when asked).
/// Operation: optional baseline write + compare.
fn handle_baseline_and_compare(cli: &Cli, analysis: &report::AnalysisResult) -> Result<(), i32> {
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
    Ok(())
}
