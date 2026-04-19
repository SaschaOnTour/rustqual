mod adapters;
mod app;
mod cli;
mod cli_handlers;
mod domain;
mod pipeline;
mod ports;
use adapters::config;
use adapters::report;
use adapters::source::watch;
use adapters::suppression::qual_allow as findings;
use cli_handlers::{handle_compare, handle_completions, handle_init, handle_save_baseline};

use clap::Parser;

use cli::{Cli, OutputFormat};
use config::Config;

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
            let scope =
                crate::adapters::analyzers::iosp::scope::ProjectScope::from_files(&scope_refs);
            let analyzer_obj =
                crate::adapters::analyzers::iosp::Analyzer::new(&default_config, &scope);
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

    if let Some(ref target) = cli.explain {
        return crate::adapters::analyzers::architecture::cli::handle_explain(target, &config);
    }

    if cli.watch {
        return watch::run_watch_mode(&cli.path, || {
            pipeline::analyze_and_output(
                &cli.path,
                &config,
                &output_format,
                cli.verbose,
                cli.suggestions,
            );
        });
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
mod tests;
