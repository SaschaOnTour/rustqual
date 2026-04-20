//! CLI-level entry points for architecture diagnostics.
//!
//! The composition-root dispatches the `--explain <file>` flag into
//! `handle_explain`, which loads the file, compiles the architecture
//! config, runs the rule checks on that single file, and prints the
//! rendered report to stdout.

use crate::adapters::analyzers::architecture::compiled::compile_architecture;
use crate::adapters::analyzers::architecture::explain::explain_file;
use crate::config::Config;
use std::path::Path;

/// Handle the --explain command: print architecture-rule diagnostics for one file.
/// Operation: orchestrates read, parse, compile, explain, render.
// qual:api
pub fn handle_explain(target: &Path, config: &Config) -> Result<(), i32> {
    let source = match std::fs::read_to_string(target) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {e}", target.display());
            return Err(1);
        }
    };
    let ast: syn::File = match syn::parse_str(&source) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error parsing {}: {e}", target.display());
            return Err(1);
        }
    };
    let compiled = match compile_architecture(&config.architecture) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error compiling [architecture] config: {e}");
            return Err(2);
        }
    };
    let rel = target.to_string_lossy().replace('\\', "/");
    let report = explain_file(&rel, &ast, &compiled);
    print!("{}", report.render());
    Ok(())
}
