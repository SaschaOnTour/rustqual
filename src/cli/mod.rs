pub(crate) mod explain;
pub(crate) mod handlers;

use std::path::PathBuf;

use clap::Parser;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    #[default]
    Text,
    Json,
    Github,
    Dot,
    Sarif,
    Html,
    Ai,
    AiJson,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "github" => Ok(Self::Github),
            "dot" => Ok(Self::Dot),
            "sarif" => Ok(Self::Sarif),
            "html" => Ok(Self::Html),
            "ai" => Ok(Self::Ai),
            "ai-json" => Ok(Self::AiJson),
            _ => Err(format!(
                "Unknown format: {s}. Expected: text, json, github, dot, sarif, html, ai, ai-json"
            )),
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "rustqual",
    about = "Structural quality analyzer for Rust — seven dimensions: IOSP, Complexity, DRY, SRP, Coupling, Test Quality, Architecture (with adapter call-parity).",
    version
)]
pub(crate) struct Cli {
    /// Path to analyze (file or directory). Defaults to current directory.
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Show all functions, not just findings.
    #[arg(short, long)]
    pub verbose: bool,

    /// Show only findings with file:line locations (one per line).
    #[arg(long)]
    pub findings: bool,

    /// Output results as JSON (for CI integration).
    #[arg(long)]
    pub json: bool,

    /// Output format: text (default), json, github, dot, sarif, html, ai, ai-json.
    #[arg(long, value_name = "FORMAT")]
    pub format: Option<OutputFormat>,

    /// Path to config file. Defaults to `rustqual.toml` in the target directory.
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Treat closures as logic (stricter analysis).
    #[arg(long)]
    pub strict_closures: bool,

    /// Treat iterator chains (.map, .filter, ...) as logic.
    #[arg(long)]
    pub strict_iterators: bool,

    /// Allow recursive calls (don't count as violations).
    #[arg(long)]
    pub allow_recursion: bool,

    /// Count ? operator as logic (implicit control flow).
    #[arg(long)]
    pub strict_error_propagation: bool,

    /// Do not exit with code 1 on quality findings (useful for local exploration).
    #[arg(long)]
    pub no_fail: bool,

    /// Treat warnings (e.g. suppression ratio exceeded) as errors (exit code 1).
    #[arg(long)]
    pub fail_on_warnings: bool,

    /// Generate a default rustqual.toml configuration file.
    #[arg(long)]
    pub init: bool,

    /// Generate shell completions for the given shell.
    #[arg(long, value_name = "SHELL")]
    pub completions: Option<clap_complete::Shell>,

    /// Save current analysis results as a baseline for future comparison.
    #[arg(long, value_name = "FILE")]
    pub save_baseline: Option<PathBuf>,

    /// Compare current results against a previously saved baseline.
    #[arg(long, value_name = "FILE")]
    pub compare: Option<PathBuf>,

    /// Return exit code 1 only if the quality score regressed compared to baseline.
    #[arg(long)]
    pub fail_on_regression: bool,

    /// Watch for file changes and re-analyze continuously.
    #[arg(long)]
    pub watch: bool,

    /// Show refactoring suggestions for IOSP violations.
    #[arg(long)]
    pub suggestions: bool,

    /// Minimum quality score (0–100). Exit code 1 if below threshold.
    #[arg(long, value_name = "SCORE")]
    pub min_quality_score: Option<f64>,

    /// Sort IOSP violations by refactoring effort (highest first).
    #[arg(long)]
    pub sort_by_effort: bool,

    /// Analyze only files changed vs a git ref (default: HEAD).
    /// Conflicts with --watch.
    #[arg(long, value_name = "REF", num_args = 0..=1, default_missing_value = "HEAD", conflicts_with = "watch")]
    pub diff: Option<String>,

    /// Path to an LCOV coverage file for test quality analysis (TQ-004, TQ-005).
    #[arg(long, value_name = "LCOV_FILE")]
    pub coverage: Option<PathBuf>,

    /// Diagnostic mode: explain architecture-rule classification for one file.
    #[arg(long, value_name = "FILE")]
    pub explain: Option<PathBuf>,
}
