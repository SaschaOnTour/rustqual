use super::sections;

/// Headroom factor applied to current maximums for tailored config thresholds.
const HEADROOM_FACTOR: f64 = 1.2;

/// Project metrics collected during init for tailored config generation.
pub struct ProjectMetrics {
    pub file_count: usize,
    pub function_count: usize,
    pub max_cognitive: usize,
    pub max_cyclomatic: usize,
    pub max_nesting_depth: usize,
    pub max_function_lines: usize,
}

/// Extract project metrics from analysis results for tailored config generation.
/// Operation: iterates results and computes max complexity values.
pub fn extract_init_metrics(
    file_count: usize,
    results: &[crate::adapters::analyzers::iosp::FunctionAnalysis],
) -> ProjectMetrics {
    let mut max_cognitive = 0usize;
    let mut max_cyclomatic = 0usize;
    let mut max_nesting_depth = 0usize;
    let mut max_function_lines = 0usize;

    for r in results {
        if let Some(ref cx) = r.complexity {
            max_cognitive = max_cognitive.max(cx.cognitive_complexity);
            max_cyclomatic = max_cyclomatic.max(cx.cyclomatic_complexity);
            max_nesting_depth = max_nesting_depth.max(cx.max_nesting);
            max_function_lines = max_function_lines.max(cx.function_lines);
        }
    }

    ProjectMetrics {
        file_count,
        function_count: results.len(),
        max_cognitive,
        max_cyclomatic,
        max_nesting_depth,
        max_function_lines,
    }
}

/// Generate a default rustqual.toml configuration file content.
/// Operation: pure string construction.
pub fn generate_default_config() -> &'static str {
    r#"# rustqual.toml — Configuration for the rustqual code quality analyzer
#
# Place this file in your project root.
# Run `rustqual --init` to generate this file.

# ── Function Classification ──────────────────────────────────────────────

# Function names (or glob patterns) to exclude from analysis.
# Examples: "main", "test_*", "visit_*"
ignore_functions = [
    "main",
    "test_*",
]

# Glob patterns for files to exclude from analysis.
# Examples: "generated/**", "tests/**"
exclude_files = []

# If true, closures count as "logic" even when passed to iterator adaptors.
# Default: false (lenient — closures inside .map()/.filter() are ignored).
strict_closures = false

# If true, iterator chains (.map, .filter, .fold, ...) count as own calls.
# Default: false.
strict_iterator_chains = false

# If true, recursive calls (function calling itself) are allowed and don't
# count as violations. Default: false.
allow_recursion = false

# If true, the ? operator counts as logic (implicit control flow).
# Default: false.
strict_error_propagation = false

# ── Suppression Health ───────────────────────────────────────────────────

# Maximum ratio of suppressed functions before a warning is emitted.
# Default: 0.05 (5%).
max_suppression_ratio = 0.05

# If true, exit with code 1 when warnings are present (e.g. suppression ratio exceeded).
# Default: false. Use --fail-on-warnings CLI flag to enable.
fail_on_warnings = false

# ── Complexity Analysis ──────────────────────────────────────────────────

[complexity]
enabled = true
max_cognitive = 15
max_cyclomatic = 10
include_nesting_penalty = true
detect_magic_numbers = true
allowed_magic_numbers = ["0", "1", "-1", "2"]

# ── DRY / Duplicate Detection ───────────────────────────────────────────

[duplicates]
enabled = true
similarity_threshold = 0.85
min_tokens = 30
min_lines = 5
min_statements = 3
ignore_tests = true
ignore_trait_impls = true
detect_dead_code = true
detect_wildcard_imports = true
detect_repeated_matches = true

# ── Boilerplate Detection ───────────────────────────────────────────────

[boilerplate]
enabled = true
# Optional: limit to specific patterns (empty = all patterns).
# patterns = ["BP-001", "BP-003"]
suggest_crates = true

# ── SRP (Single Responsibility) ─────────────────────────────────────────

[srp]
enabled = true
smell_threshold = 0.6
max_fields = 12
max_methods = 20
max_fan_out = 10
lcom4_threshold = 2
weights = [0.4, 0.25, 0.15, 0.2]
file_length_baseline = 300
file_length_ceiling = 800
max_independent_clusters = 3
min_cluster_statements = 5
# Maximum number of parameters before a function triggers SRP-004.
max_parameters = 5

# ── Coupling Analysis ───────────────────────────────────────────────────

[coupling]
enabled = true
max_instability = 0.8
max_fan_in = 15
max_fan_out = 12
# Check Stable Dependencies Principle (stable modules should not depend on unstable ones).
check_sdp = true

# ── Test Quality Analysis ──────────────────────────────────────────────

[test_quality]
enabled = true
# Optional: path to LCOV coverage file for TQ-004/TQ-005 checks.
# coverage_file = "lcov.info"

# ── Quality Score Weights ──────────────────────────────────────────────
# Weights for each dimension in the overall quality score.
# Must sum to approximately 1.0.

[weights]
iosp         = 0.22
complexity   = 0.18
dry          = 0.13
srp          = 0.18
coupling     = 0.09
test_quality = 0.10
architecture = 0.10
"#
}

/// Compute tailored thresholds: current max + headroom, at least the default.
/// Operation: arithmetic + comparison logic.
fn compute_tailored_thresholds(m: &ProjectMetrics) -> [usize; 4] {
    let cognitive = ((m.max_cognitive as f64 * HEADROOM_FACTOR).ceil() as usize)
        .max(sections::DEFAULT_MAX_COGNITIVE);
    let cyclomatic = ((m.max_cyclomatic as f64 * HEADROOM_FACTOR).ceil() as usize)
        .max(sections::DEFAULT_MAX_CYCLOMATIC);
    let nesting = ((m.max_nesting_depth as f64 * HEADROOM_FACTOR).ceil() as usize)
        .max(sections::DEFAULT_MAX_NESTING_DEPTH);
    let function_lines = ((m.max_function_lines as f64 * HEADROOM_FACTOR).ceil() as usize)
        .max(sections::DEFAULT_MAX_FUNCTION_LINES);
    [cognitive, cyclomatic, nesting, function_lines]
}

/// Format a tailored rustqual.toml from metrics and computed thresholds.
/// Trivial: pure string formatting (format! macro, no logic or own calls).
fn format_tailored_config(m: &ProjectMetrics, thresholds: &[usize; 4]) -> String {
    let [cognitive, cyclomatic, nesting, function_lines] = *thresholds;
    format!(
        r#"# rustqual.toml — Tailored configuration for your project
# Generated from analysis of {file_count} file(s), {function_count} function(s).
#
# Thresholds are set to your current maximums + 20% headroom.
# Tighten them over time as you improve code quality.

# ── Function Classification ──────────────────────────────────────────────

ignore_functions = ["main", "test_*"]
exclude_files = []
strict_closures = false
strict_iterator_chains = false
allow_recursion = false
strict_error_propagation = false

# ── Suppression Health ───────────────────────────────────────────────────

max_suppression_ratio = 0.05
fail_on_warnings = false

# ── Complexity Analysis ──────────────────────────────────────────────────

[complexity]
enabled = true
max_cognitive = {cognitive}           # current max: {max_cog}
max_cyclomatic = {cyclomatic}          # current max: {max_cyc}
max_nesting_depth = {nesting}            # current max: {max_nest}
max_function_lines = {function_lines}         # current max: {max_lines}
include_nesting_penalty = true
detect_magic_numbers = true
detect_unsafe = true
detect_error_handling = true
allowed_magic_numbers = ["0", "1", "-1", "2"]

# ── DRY / Duplicate Detection ───────────────────────────────────────────

[duplicates]
enabled = true
similarity_threshold = 0.85
min_tokens = 30
min_lines = 5
min_statements = 3
ignore_tests = true
ignore_trait_impls = true
detect_dead_code = true
detect_wildcard_imports = true
detect_repeated_matches = true

# ── Boilerplate Detection ───────────────────────────────────────────────

[boilerplate]
enabled = true
suggest_crates = true

# ── SRP (Single Responsibility) ─────────────────────────────────────────

[srp]
enabled = true
smell_threshold = 0.6
max_fields = 12
max_methods = 20
max_fan_out = 10
lcom4_threshold = 2
weights = [0.4, 0.25, 0.15, 0.2]
file_length_baseline = 300
file_length_ceiling = 800
max_independent_clusters = 3
min_cluster_statements = 5
max_parameters = 5

# ── Coupling Analysis ───────────────────────────────────────────────────

[coupling]
enabled = true
max_instability = 0.8
max_fan_in = 15
max_fan_out = 12
check_sdp = true

# ── Test Quality Analysis ──────────────────────────────────────────────

[test_quality]
enabled = true
# coverage_file = "lcov.info"

# ── Quality Score Weights ──────────────────────────────────────────────
# Must sum to approximately 1.0.

[weights]
iosp         = 0.22
complexity   = 0.18
dry          = 0.13
srp          = 0.18
coupling     = 0.09
test_quality = 0.10
architecture = 0.10
"#,
        file_count = m.file_count,
        function_count = m.function_count,
        max_cog = m.max_cognitive,
        max_cyc = m.max_cyclomatic,
        max_nest = m.max_nesting_depth,
        max_lines = m.max_function_lines,
    )
}

/// Generate a tailored rustqual.toml based on project metrics.
/// Integration: orchestrates threshold computation and formatting.
pub fn generate_tailored_config(m: &ProjectMetrics) -> String {
    let thresholds = compute_tailored_thresholds(m);
    format_tailored_config(m, &thresholds)
}
