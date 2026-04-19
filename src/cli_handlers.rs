//! Small, single-purpose handlers dispatched by the composition root.
//!
//! Extracted out of `lib.rs` to keep the entry-point file compact. Each
//! handler is one of the CLI surface's narrow modes (`--init`,
//! `--completions`, `--save-baseline`, `--compare`). They live here
//! rather than under `src/app/` because they read and write stdout/stderr
//! directly and so belong with the other composition-root-adjacent code.

use crate::adapters::analyzers::iosp::FunctionAnalysis;
use crate::cli::Cli;
use crate::report;
use clap::CommandFactory;
use std::path::Path;

/// Handle the --init command: write a rustqual.toml config file.
/// Operation: checks file existence and writes.
pub(crate) fn handle_init(content: &str) -> Result<(), i32> {
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
pub(crate) fn handle_completions(shell: clap_complete::Shell) {
    clap_complete::generate(
        shell,
        &mut Cli::command(),
        "rustqual",
        &mut std::io::stdout(),
    );
}

/// Handle --save-baseline: write results to a JSON file.
/// Operation: serialization + file write logic.
pub(crate) fn handle_save_baseline(
    path: &Path,
    all_results: &[FunctionAnalysis],
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
pub(crate) fn handle_compare(
    path: &Path,
    all_results: &[FunctionAnalysis],
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
