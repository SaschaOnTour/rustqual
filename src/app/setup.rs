//! Use-case: load, compile, and finalise a `Config` from CLI input.
//!
//! `setup_config` is the orchestrator the composition root calls right
//! after CLI parsing. It loads the file (explicit path or auto-discovered),
//! pre-compiles globs, folds CLI overrides into the struct, and runs the
//! cross-field validation. All errors are translated into the caller's
//! preferred exit-code shape (`Result<_, i32>`).

use crate::cli::Cli;
use crate::config::{self, Config};
use std::path::Path;

/// Load, compile, and apply CLI overrides to config.
/// Integration: delegates to load, compile, override, validate helpers.
pub(crate) fn setup_config(cli: &Cli) -> Result<Config, i32> {
    let mut config = load_config(cli)?;
    config.compile();
    apply_cli_overrides(&mut config, cli);
    validate_config_weights(&config)?;
    Ok(config)
}

/// Load configuration from CLI args or auto-discovery.
/// Integration: delegates to load_explicit_config or load_auto_config.
fn load_config(cli: &Cli) -> Result<Config, i32> {
    cli.config
        .as_ref()
        .map(|p| load_explicit_config(p))
        .unwrap_or_else(|| load_auto_config(&cli.path))
}

/// Print `Error: {e}` on stderr and map to exit code 2.
/// Operation: I/O + value, no own calls.
fn report_config_error<E: std::fmt::Display>(e: E) -> i32 {
    eprintln!("Error: {e}");
    2
}

/// Load config from an explicit config file path.
/// Trivial: single delegation to load_from_file.
fn load_explicit_config(config_path: &Path) -> Result<Config, i32> {
    Config::load_from_file(config_path).map_err(report_config_error)
}

/// Load config via auto-discovery from the project path.
/// Trivial: single delegation to load.
fn load_auto_config(path: &Path) -> Result<Config, i32> {
    Config::load(path).map_err(report_config_error)
}

/// Validate config settings that require cross-field checks.
/// Trivial: single delegation to validate_weights.
fn validate_config_weights(config: &Config) -> Result<(), i32> {
    config::validate_weights(config).map_err(report_config_error)
}

/// Apply CLI flag overrides to config.
/// Operation: conditional logic on CLI flags.
pub(crate) fn apply_cli_overrides(config: &mut Config, cli: &Cli) {
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
        config.test_quality.coverage_file = Some(coverage.display().to_string());
    }
}
