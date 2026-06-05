//! CLI-level entry points for the `--explain` flag.
//!
//! Two entry points share this module:
//! - `--explain <file>` → `handle_explain`, which loads the file, compiles the
//!   architecture config, runs the rule checks on that single file, and prints
//!   the rendered report to stdout.
//! - `--explain allow` → `explain_allow` / `suppression_guide`, which print the
//!   `// qual:allow(...)` suppression guide (grammar + per-dimension targets
//!   sourced from the shared vocabulary + the pin/orphan rationale).

use crate::adapters::analyzers::architecture::compiled::compile_architecture;
use crate::adapters::analyzers::architecture::explain::explain_file;
use crate::config::Config;
use crate::domain::{target_kind, target_names, Dimension, TargetKind};
use std::path::Path;

/// Print the `// qual:allow(...)` suppression guide (`rustqual --explain allow`).
/// Operation: delegates to the testable builder + prints.
// qual:api
pub fn explain_allow() {
    print!("{}", suppression_guide());
}

const GUIDE_HEADER: &str = "\nqual:allow — targeted suppressions\n\n\
Suppress ONE finding-kind, not a whole dimension:\n\
\x20 // qual:allow(dim, target)        boolean target (takes no value)\n\
\x20 // qual:allow(dim, target=N)      metric target — value REQUIRED; re-fires above N\n\
\x20 // qual:allow(dim)                whole dimension (multi-kind dims reject this)\n\n\
Every targeted suppression needs a reason:\n\
\x20 // qual:allow(srp, file_length=400) reason: \"long but cohesive\"\n\n\
Valid targets per dimension:\n";

const GUIDE_FOOTER: &str =
    "\nA metric pin re-fires once the value grows past it, and is flagged too-loose\n\
(orphan) when the pin sits more than [suppression].pin_headroom above the value\n\
it covers (default 10%) — so a pin can never silently mask a growing problem. A\n\
targeted marker is also orphaned when no finding of its kind exists. Fix the\n\
finding first; suppression is the last resort.\n";

/// Build the suppression guide as a string (so it can be tested): grammar,
/// per-dimension targets sourced from the shared vocabulary, and the rationale.
/// Operation: header + per-dimension blocks + footer via a closure.
pub(crate) fn suppression_guide() -> String {
    let mut out = String::from(GUIDE_HEADER);
    let dims = [
        Dimension::Iosp,
        Dimension::Complexity,
        Dimension::Dry,
        Dimension::Srp,
        Dimension::Coupling,
        Dimension::TestQuality,
        Dimension::Architecture,
    ];
    dims.iter()
        .for_each(|&dim| out.push_str(&dim_targets_block(dim)));
    out.push_str(GUIDE_FOOTER);
    out
}

/// One dimension's block: its metric (pinnable) and boolean targets, or a
/// note that it is single-kind (bare `allow(dim)` only).
/// Operation: builds the lines from the shared vocabulary.
fn dim_targets_block(dim: Dimension) -> String {
    let metrics = targets_of_kind(dim, TargetKind::Metric);
    let booleans = targets_of_kind(dim, TargetKind::Boolean);
    if metrics.is_empty() && booleans.is_empty() {
        return format!("  {dim}: bare allow({dim}) only (single-kind)\n");
    }
    let mut block = format!("  {dim}:\n");
    if !metrics.is_empty() {
        block.push_str(&format!(
            "      metric (needs =N): {}\n",
            metrics.join(", ")
        ));
    }
    if !booleans.is_empty() {
        block.push_str(&format!(
            "      boolean:           {}\n",
            booleans.join(", ")
        ));
    }
    block
}

/// The target names of `dim` whose kind is `kind`.
/// Operation: filter over the shared vocabulary.
fn targets_of_kind(dim: Dimension, kind: TargetKind) -> Vec<&'static str> {
    target_names(dim)
        .iter()
        .copied()
        .filter(|t| target_kind(dim, t) == Some(kind))
        .collect()
}

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

#[cfg(test)]
mod tests;
