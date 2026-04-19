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
