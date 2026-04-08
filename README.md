# rustqual

[![CI](https://github.com/SaschaOnTour/rustqual/actions/workflows/ci.yml/badge.svg)](https://github.com/SaschaOnTour/rustqual/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rustqual.svg)](https://crates.io/crates/rustqual)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

Comprehensive Rust code quality analyzer — six dimensions: Complexity, Coupling, DRY, IOSP, SRP, Test Quality — plus 7 structural binary checks integrated into SRP and Coupling. Particularly useful as a structural quality guardrail for AI-generated code, catching the god-functions, mixed concerns, duplicated patterns, and weak tests that AI coding agents commonly produce.

## Quality Dimensions

rustqual analyzes your Rust code across **six quality dimensions**, each contributing to an overall quality score:

| Dimension    | Weight | What it checks |
|--------------|--------|----------------|
| IOSP         | 25%    | Function separation (Integration vs Operation) |
| Complexity   | 20%    | Cognitive/cyclomatic complexity, magic numbers, nesting depth, function length, unsafe blocks, error handling |
| DRY          | 20%    | Duplicate functions, fragments, dead code, boilerplate |
| SRP          | 15%    | Struct cohesion (LCOM4), module length, function clusters, structural checks (BTC, SLM, NMS) |
| Test Quality | 10%    | Assertion density (TQ-001), test function length (TQ-002), mock-heavy tests (TQ-003), assertion-free tests (TQ-004), coverage gaps (TQ-005) |
| Coupling     | 10%    | Module instability, circular dependencies, SDP, structural checks (OI, SIT, DEH, IET) |

## What is IOSP?

The **Integration Operation Segregation Principle** (from Ralf Westphal's *Flow Design*) states that every function should be **either**:

- **Integration** — orchestrates other functions, contains no logic of its own
- **Operation** — contains logic (control flow, computation), but does not call other "own" functions

A function that does **both** is a **violation**. A function too small to matter (empty body, single expression without logic or own calls) is classified as **Trivial**.

```
┌─────────────┐     ┌─────────────┐     ┌────────────────────┐
│ Integration │     │  Operation  │     │    ✗ Violation     │
│             │     │             │     │                    │
│ calls A()   │     │ if x > 0   │     │ if x > 0           │
│ calls B()   │     │   y = x*2  │     │   result = calc()  │ ← mixes both
│ calls C()   │     │ return y   │     │ return result + 1  │
└─────────────┘     └─────────────┘     └────────────────────┘
```

## Installation

```bash
# From crates.io
cargo install rustqual

# From source
cargo install --path .

# Then use either:
rustqual src/            # direct invocation
cargo qual src/          # as cargo subcommand
```

## Quick Start

```bash
# Analyze current directory
rustqual

# Analyze a specific file or directory
rustqual src/lib.rs
rustqual src/

# Show all functions, not just findings
rustqual --verbose

# Do not exit with code 1 on findings (for local exploration)
rustqual --no-fail

# Generate a default config file
rustqual --init

# Watch mode: re-analyze on file changes
rustqual --watch src/
```

> **Using AI coding agents?** See [Using with AI Coding Agents](#using-with-ai-coding-agents) for integration patterns with Claude Code, Cursor, Copilot, and other tools.

## Output Formats

### Text (default)

```bash
rustqual src/ --verbose
```

```
── src/order.rs
  ✓ INTEGRATION process_order (line 12)
  ✓ OPERATION   calculate_discount (line 28)
    Complexity: logic=2, calls=0, nesting=1, cognitive=2, cyclomatic=3
  ✗ VIOLATION   process_payment (line 48) [MEDIUM]
    Logic: if (line 50), comparison (line 50), if (line 56)
    Calls: determine_payment_method (line 55), charge_credit_card (line 59)
    Complexity: logic=3, calls=2, nesting=1, cognitive=5, cyclomatic=4
  · TRIVIAL     get_name (line 72)
  ~ SUPPRESSED  legacy_handler (line 85)

═══ Summary ═══
  Functions: 24    Quality Score: 82.3%

  IOSP:           85.7%  (4I, 8O, 10T, 2 violations)
  Complexity:     90.0%  (3 complexity, 1 magic numbers)
  DRY:            95.0%  (1 duplicates, 2 dead code)
  SRP:           100.0%
  Test Quality:  100.0%
  Coupling:      100.0%

  ~ Suppressed:   1

4 quality findings. Run with --verbose for details.
```

### JSON

```bash
rustqual --json
# or
rustqual --format json
```

Produces machine-readable output with `summary`, `functions`, `coupling`, `duplicates`, `dead_code`, `fragments`, `boilerplate`, and `srp` sections:

```json
{
  "summary": {
    "total": 24,
    "integrations": 4,
    "operations": 8,
    "violations": 2,
    "trivial": 10,
    "suppressed": 1,
    "iosp_score": 0.857,
    "quality_score": 0.823,
    "coupling_warnings": 0,
    "coupling_cycles": 0,
    "duplicate_groups": 0,
    "dead_code_warnings": 0,
    "fragment_groups": 0,
    "boilerplate_warnings": 0,
    "srp_struct_warnings": 0,
    "srp_module_warnings": 0,
    "suppression_ratio_exceeded": false
  },
  "functions": [...]
}
```

### GitHub Actions Annotations

```bash
rustqual --format github
```

Produces `::warning`, `::error`, and `::notice` annotations that GitHub Actions renders inline on PRs:

```
::warning file=src/order.rs,line=48::IOSP violation in process_payment: logic=[if (line 50)], calls=[determine_payment_method (line 55)]
::error::Quality analysis: 2 violation(s), 82.3% quality score
```

### DOT (Graphviz)

```bash
rustqual --format dot > call-graph.dot
dot -Tsvg call-graph.dot -o call-graph.svg
```

Generates a call-graph visualization with color-coded nodes:
- Green: Integration
- Blue: Operation
- Red: Violation
- Gray: Trivial

### SARIF

```bash
rustqual --format sarif > report.sarif
```

Produces [SARIF v2.1.0](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html) output for integration with GitHub Code Scanning, VS Code SARIF Viewer, and other static analysis platforms. Includes rules for all dimensions (IOSP, complexity, coupling, DRY, SRP, test quality).

### HTML

```bash
rustqual --format html > report.html
```

Generates a self-contained HTML report with:
- Dashboard showing overall quality score and 6 dimension scores
- Collapsible detail sections for IOSP, Complexity, DRY, SRP, Test Quality, and Coupling findings
- Color-coded severity indicators and inline CSS (no external dependencies)

## CLI Reference

```
rustqual [OPTIONS] [PATH]
```

| Argument / Flag                 | Description                                                          |
|---------------------------------|----------------------------------------------------------------------|
| `PATH`                          | File or directory to analyze. Defaults to `.`                        |
| `-v, --verbose`                 | Show all functions, not just findings                                |
| `--json`                        | Output as JSON (shorthand for `--format json`)                       |
| `--format <FORMAT>`             | Output format: `text`, `json`, `github`, `dot`, `sarif`, `html`      |
| `-c, --config <PATH>`           | Path to config file. Defaults to auto-discovered `rustqual.toml`     |
| `--strict-closures`             | Treat closures as logic (stricter analysis)                          |
| `--strict-iterators`            | Treat iterator chains (`.map`, `.filter`, ...) as logic              |
| `--allow-recursion`             | Don't count recursive calls as violations                           |
| `--strict-error-propagation`    | Count `?` operator as logic (implicit control flow)                  |
| `--no-fail`                     | Do not exit with code 1 on quality findings (local exploration)      |
| `--fail-on-warnings`            | Treat warnings (e.g. suppression ratio exceeded) as errors (exit 1)  |
| `--init`                        | Generate a tailored `rustqual.toml` based on current codebase metrics |
| `--completions <SHELL>`         | Generate shell completions (bash, zsh, fish, elvish, powershell)     |
| `--save-baseline <FILE>`        | Save current results as a JSON baseline                              |
| `--compare <FILE>`              | Compare current results against a saved baseline                     |
| `--fail-on-regression`          | Exit with code 1 if quality score regressed vs baseline              |
| `--watch`                       | Watch for file changes and re-analyze continuously                   |
| `--suggestions`                 | Show refactoring suggestions for IOSP violations                     |
| `--sort-by-effort`              | Sort violations by refactoring effort score (descending)             |
| `--findings`                    | Show only findings with `file:line` locations (one per line)         |
| `--min-quality-score <SCORE>`   | Exit with code 1 if quality score is below threshold (0–100)         |
| `--diff [REF]`                  | Only analyze files changed vs a git ref (default: HEAD)              |
| `--coverage <LCOV_FILE>`        | Path to LCOV coverage file for test quality analysis (TQ-005)        |

### Exit Codes

| Code | Meaning                                     |
|------|---------------------------------------------|
| `0`  | Success (no findings, or `--no-fail` set)   |
| `1`  | Quality findings found (default), regression detected (`--fail-on-regression`), quality gate breached (`--min-quality-score`), or warnings present with `--fail-on-warnings` |
| `2`  | Configuration error (invalid or unreadable config file)  |

## Configuration

The analyzer auto-discovers `rustqual.toml` by searching from the analysis path upward through parent directories. You can also specify a config explicitly with `--config`. Generate a commented default config with `--init`.

If a `rustqual.toml` exists but cannot be parsed (syntax errors, unknown fields), the analyzer exits with code 2 and an error message instead of silently falling back to defaults.

### Full `rustqual.toml` Reference

```toml
# ────────────────────────────────────────────────────────────────
# External Prefixes
# ────────────────────────────────────────────────────────────────
# Calls to these crate/module prefixes are NOT counted as "own" calls.
external_prefixes = [
    "std", "core", "alloc", "log", "tracing", "anyhow", "thiserror",
    "serde", "tokio", "println", "eprintln", "format", "vec", "dbg",
    "todo", "unimplemented", "panic", "assert", "assert_eq", "assert_ne",
    "debug_assert",
]

# ────────────────────────────────────────────────────────────────
# Ignore Functions
# ────────────────────────────────────────────────────────────────
# Functions matching these patterns are completely excluded from analysis.
# Supports full glob syntax: *, ?, [abc], [!abc]
ignore_functions = [
    "main",      # entry point, always mixes logic + calls
    "test_*",    # test functions
    "visit_*",   # syn::Visit trait implementations
]

# ────────────────────────────────────────────────────────────────
# Exclude Files
# ────────────────────────────────────────────────────────────────
# Glob patterns for files to exclude from analysis entirely.
exclude_files = []

# ────────────────────────────────────────────────────────────────
# Strictness
# ────────────────────────────────────────────────────────────────
strict_closures = false              # If true, closures count as logic
strict_iterator_chains = false       # If true, iterator chains count as own calls
allow_recursion = false              # If true, recursive calls don't violate IOSP
strict_error_propagation = false     # If true, ? operator counts as logic

# ────────────────────────────────────────────────────────────────
# Suppression Ratio
# ────────────────────────────────────────────────────────────────
# Maximum fraction of functions that may be suppressed (0.0–1.0).
# Exceeding this ratio produces a warning.
max_suppression_ratio = 0.05

# If true, exit with code 1 when warnings are present (e.g. suppression ratio exceeded).
# Default: false. Use --fail-on-warnings CLI flag to enable.
fail_on_warnings = false

# ────────────────────────────────────────────────────────────────
# Complexity Analysis
# ────────────────────────────────────────────────────────────────
[complexity]
enabled = true
max_cognitive = 15                   # Cognitive complexity threshold
max_cyclomatic = 10                  # Cyclomatic complexity threshold
max_nesting_depth = 4                # Maximum nesting depth before warning
max_function_lines = 60              # Maximum function body lines before warning
detect_magic_numbers = true          # Flag numeric literals not in allowed list
allowed_magic_numbers = ["0", "1", "-1", "2", "0.0", "1.0"]
detect_unsafe = true                 # Flag functions containing unsafe blocks
detect_error_handling = true         # Flag unwrap/expect/panic/todo usage
allow_expect = false                 # If true, .expect() calls don't trigger warnings

# ────────────────────────────────────────────────────────────────
# Coupling Analysis
# ────────────────────────────────────────────────────────────────
[coupling]
enabled = true
max_instability = 0.8                # Instability threshold (Ce / (Ca + Ce))
max_fan_in = 15                      # Maximum afferent coupling
max_fan_out = 12                     # Maximum efferent coupling
check_sdp = true                     # Check Stable Dependencies Principle

# ────────────────────────────────────────────────────────────────
# DRY / Duplicate Detection
# ────────────────────────────────────────────────────────────────
[duplicates]
enabled = true
min_tokens = 50                      # Minimum token count for duplicate detection
min_lines = 5                        # Minimum line count
min_statements = 3                   # Minimum statements for fragment detection
similarity_threshold = 0.85          # Jaccard similarity for near-duplicates
ignore_tests = true                  # Skip test functions
detect_dead_code = true              # Enable dead code detection
detect_wildcard_imports = true       # Flag use foo::* imports
detect_repeated_matches = true      # Flag repeated match blocks (DRY-005)

# ────────────────────────────────────────────────────────────────
# Boilerplate Detection
# ────────────────────────────────────────────────────────────────
[boilerplate]
enabled = true
suggest_crates = true                # Suggest derive macros / crates
patterns = [                         # Which patterns to check (BP-001 through BP-010)
    "BP-001", "BP-002", "BP-003", "BP-004", "BP-005",
    "BP-006", "BP-007", "BP-008", "BP-009", "BP-010",
]

# ────────────────────────────────────────────────────────────────
# SRP Analysis
# ────────────────────────────────────────────────────────────────
[srp]
enabled = true
smell_threshold = 0.6                # Composite score threshold for warnings
max_fields = 12                      # Maximum struct fields
max_methods = 15                     # Maximum impl methods
max_fan_out = 10                     # Maximum external call targets
max_parameters = 5                   # Maximum function parameters (AST-based)
lcom4_threshold = 3                  # LCOM4 component threshold
weights = [0.4, 0.25, 0.15, 0.2]    # [lcom4, fields, methods, fan_out]
file_length_baseline = 300           # Production lines before penalty starts
file_length_ceiling = 800            # Production lines at maximum penalty
max_independent_clusters = 3         # Max independent function groups before warning
min_cluster_statements = 5           # Min statements for a function to count in clusters

# ────────────────────────────────────────────────────────────────
# Structural Binary Checks
# ────────────────────────────────────────────────────────────────
[structural]
enabled = true
check_btc = true                     # Broken Trait Contract (SRP)
check_slm = true                     # Self-less Methods (SRP)
check_nms = true                     # Needless &mut self (SRP)
check_oi = true                      # Orphaned Impl (Coupling)
check_sit = true                     # Single-Impl Trait (Coupling)
check_deh = true                     # Downcast Escape Hatch (Coupling)
check_iet = true                     # Inconsistent Error Types (Coupling)

# ────────────────────────────────────────────────────────────────
# Test Quality Analysis
# ────────────────────────────────────────────────────────────────
[test]
enabled = true
coverage_file = ""                   # Path to LCOV file (or use --coverage CLI flag)

# ────────────────────────────────────────────────────────────────
# Quality Weights
# ────────────────────────────────────────────────────────────────
[weights]
iosp = 0.25                          # Weight for IOSP dimension
complexity = 0.20                    # Weight for Complexity dimension
dry = 0.20                           # Weight for DRY dimension
srp = 0.15                           # Weight for SRP dimension
test_quality = 0.10                  # Weight for Test Quality dimension
coupling = 0.10                      # Weight for Coupling dimension
# Weights must sum to 1.0
```

### Inline Suppression

To suppress specific findings, add a `// qual:allow` comment on or immediately before the function definition:

```rust
// qual:allow
fn intentional_violation() {
    if condition {
        helper();
    }
}

// qual:allow(iosp) reason: "legacy code, scheduled for refactoring"
fn legacy_handler() { ... }

// qual:allow(complexity)
fn complex_but_justified() { ... }

// qual:allow(srp)
// #[derive(Debug, Clone)]
struct LargeButJustified { ... }
```

Supported dimensions: `iosp`, `complexity`, `coupling`, `srp`, `dry`, `test_quality`.

The legacy `// iosp:allow` syntax is still supported as an alias for `// qual:allow(iosp)`.

Suppressed functions appear as `SUPPRESSED` in the output and do not count toward findings. If more than `max_suppression_ratio` (default 5%) of functions are suppressed, a warning is displayed.

### API Annotation

Mark public API functions with `// qual:api` to exclude them from dead code (DRY-003) and untested function (TQ-003) detection:

```rust
// qual:api
pub fn encode(data: &[f32], config: &Config) -> Result<Vec<u8>> {
    // ...
}

// qual:api
pub fn decode(data: &[u8], config: &Config) -> Result<Vec<f32>> {
    // ...
}
```

Unlike `// qual:allow`, API markers do **not** count against the suppression ratio. Use `// qual:api` for functions that are part of your library's public interface — they have no callers within the project because they're meant to be called by external consumers.

### Inverse Annotation

Mark inverse method pairs with `// qual:inverse(fn_name)` to suppress near-duplicate DRY findings between them:

```rust
// qual:inverse(parse)
pub fn as_str(&self) -> &str {
    match self {
        Self::Function => "fn",
        Self::Method => "method",
        // ...
    }
}

// qual:inverse(as_str)
pub fn parse(s: &str) -> Self {
    match s {
        "fn" => Self::Function,
        "method" => Self::Method,
        // ...
    }
}
```

Common use cases: `serialize`/`deserialize`, `encode`/`decode`, `to_bytes`/`from_bytes`. Like `// qual:api`, inverse markers do **not** count against the suppression ratio — they document intentional structural similarity.

### Automatic Leaf Detection

Functions with no own calls (Operations and Trivials) are automatically recognized as **leaf functions**. Calls to leaves do not count as "own calls" for the caller:

```rust
fn get_config() -> Config {          // Operation (C=0) → leaf
    if let Ok(c) = load_file() { c } else { Config::default() }
}

fn cmd_quality(clear: bool) -> Result<()> {
    let config = get_config();       // calling a leaf → not an own call
    if clear { /* logic */ }         // logic only → Operation, not Violation
    Ok(())
}
```

Without leaf detection, `cmd_quality` would be a Violation (logic + own call). With it, the call to `get_config` is recognized as terminal — no orchestration involved.

Leaf detection cascades: if a function calls only leaves, it becomes an Operation (and thus a leaf itself), benefiting its callers.

### Lenient vs. Strict Mode

By default the analyzer runs in **lenient mode**. This makes it practical for idiomatic Rust code:

| Construct                       | Lenient (default)       | `--strict-closures`      | `--strict-iterators`    |
|---------------------------------|-------------------------|--------------------------|-------------------------|
| `items.iter().map(\|x\| x + 1)` | ignored entirely        | closure logic counted    | `.map()` as own call    |
| `\|\| { if cond { a } }`        | closure logic ignored   | `if` counted as logic    | —                       |
| `self.do_work()` in closure     | call ignored            | call counted as own      | —                       |
| `x?`                            | not logic               | —                        | —                       |
| `async { if x { } }`           | ignored (like closures) | —                        | —                       |

Use `--strict-error-propagation` to count `?` as logic.

## Features

### Quality Score

The overall quality score is a weighted average of six dimension scores (weights are configurable via `[weights]` in `rustqual.toml`):

| Dimension    | Default Weight | Metric |
|--------------|----------------|--------|
| IOSP         | 25%            | Compliance ratio (non-trivial functions) |
| Complexity   | 20%            | 1 - (complexity + magic numbers + nesting + length + unsafe + error handling) / total |
| DRY          | 20%            | 1 - (duplicates + fragments + dead code + boilerplate + wildcards + repeated matches) / total |
| SRP          | 15%            | 1 - (struct warnings + module warnings + param warnings + structural BTC/SLM/NMS) / total |
| Test Quality | 10%            | 1 - (assertion density + test length + mock-heavy + assertion-free + coverage gap) / total |
| Coupling     | 10%            | 1 - (coupling warnings + 2×cycles + SDP violations + structural OI/SIT/DEH/IET) / total |

Quality score ranges from 0% (all findings) to 100% (no findings). Weights must sum to 1.0.

### Quality Gates

By default, the analyzer exits with code 1 on any findings — no extra flags needed for CI. Use `--no-fail` for local exploration.

```bash
# Fail if quality score is below 90%
rustqual src/ --min-quality-score 90

# Local exploration (never fail)
rustqual src/ --no-fail
```

### Violation Severity

Violations are categorized by severity based on the number of findings:

| Severity | Condition          |
|----------|--------------------|
| Low      | ≤2 total findings  |
| Medium   | 3–5 total findings |
| High     | >5 total findings  |

Severity is shown as `[LOW]`, `[MEDIUM]`, `[HIGH]` in text output and as a `severity` field in JSON/SARIF.

### Complexity Metrics

Each analyzed function gets complexity metrics (shown with `--verbose`):

- **cognitive_complexity**: Cognitive complexity score (increments for nesting depth)
- **cyclomatic_complexity**: Cyclomatic complexity score (decision points + 1)
- **magic_numbers**: Numeric literals not in the configured allowed list
- **logic_count**: Number of logic occurrences (if, match, operators, etc.)
- **call_count**: Number of own-function calls
- **max_nesting**: Maximum nesting depth of control flow
- **function_lines**: Number of lines in the function body
- **unsafe_blocks**: Count of `unsafe` blocks
- **unwrap/expect/panic/todo**: Error handling pattern counts

### Coupling Analysis

Detects module-level coupling issues:

- **Afferent coupling (Ca)**: Modules depending on this one (fan-in)
- **Efferent coupling (Ce)**: Modules this one depends on (fan-out)
- **Instability**: Ce / (Ca + Ce), ranging from 0.0 (stable) to 1.0 (unstable)
- **Circular dependencies**: Detected via Kosaraju's iterative SCC algorithm

Leaf modules (Ca=0) are excluded from instability warnings since I=1.0 is natural for them.

- **Stable Dependencies Principle (SDP)**: Flags when a stable module (low instability) depends on a more unstable module. This violates the principle that dependencies should flow toward stability.

### DRY Analysis

Detects five categories of repetition:

- **Duplicate functions**: Exact and near-duplicate functions (via AST normalization + Jaccard similarity)
- **Duplicate fragments**: Repeated statement sequences across functions (sliding window + merge)
- **Dead code**: Functions never called from production code, or only called from tests. Detects both direct calls and function references passed as arguments (e.g., `.for_each(some_fn)`).
- **Boilerplate patterns**: 10 common Rust boilerplate patterns (BP-001 through BP-010) including trivial `From`/`Display` impls, manual getters/setters, builder patterns, manual `Default`, repetitive match arms, error enum boilerplate, and clone-heavy conversions
- **Wildcard imports**: Flags `use foo::*` glob imports (excludes `prelude::*` paths and `use super::*` in test modules)
- **Repeated match patterns** (DRY-005): Detects identical `match` blocks (≥3 arms) duplicated across ≥3 instances in ≥2 functions, via AST normalization and structural hashing

### SRP Analysis

Detects Single Responsibility Principle violations at three levels:

- **Struct-level**: LCOM4 cohesion analysis using Union-Find on method→field access graph. Composite score combines normalized LCOM4, field count, method count, and fan-out with configurable weights.
- **Module-level (length)**: Production line counting (before `#[cfg(test)]`) with linear penalty between configurable baseline and ceiling.
- **Module-level (cohesion)**: Detects files with too many independent function clusters. Uses Union-Find on private substantive functions, leveraging IOSP own-call data. Functions that call each other or share a common caller are united into the same cluster. A file with ≥`max_independent_clusters` (default 3) independent groups indicates multiple responsibilities that should be split into separate modules.

### Structural Binary Checks

Seven binary (pass/fail) checks for common Rust structural issues, integrated into existing dimensions:

| Rule | Name | Dimension | What it checks |
|------|------|-----------|----------------|
| BTC  | Broken Trait Contract | SRP | Impl blocks missing required trait methods |
| SLM  | Self-less Methods | SRP | Methods in impl blocks that don't use `self` (could be free functions) |
| NMS  | Needless `&mut self` | SRP | Methods taking `&mut self` that only read from self |
| OI   | Orphaned Impl | Coupling | Impl blocks in files that don't define the implemented type |
| SIT  | Single-Impl Trait | Coupling | Traits with exactly one implementation (unnecessary abstraction) |
| DEH  | Downcast Escape Hatch | Coupling | `.downcast_ref()` / `.downcast_mut()` / `.downcast()` usage (broken abstraction) |
| IET  | Inconsistent Error Types | Coupling | Modules returning 3+ different error types (missing unified error type) |

Each rule can be individually toggled via `[structural]` config. Suppress with `// qual:allow(srp)` or `// qual:allow(coupling)` depending on the dimension.

### Baseline Comparison

Track quality over time:

```bash
# Save current state as baseline
rustqual src/ --save-baseline baseline.json

# ... make changes ...

# Compare against baseline (shows new/fixed findings, score delta)
rustqual src/ --compare baseline.json

# Fail CI only on regression
rustqual src/ --compare baseline.json --fail-on-regression
```

The baseline format (v2) includes quality score, all dimension counts, and total findings. V1 baselines (IOSP-only) are still supported for backward compatibility.

### Refactoring Suggestions

```bash
rustqual src/ --suggestions
```

Provides pattern-based refactoring hints for violations, such as extracting conditions, splitting dispatch logic, or converting loops to iterator chains.

### Watch Mode

```bash
rustqual src/ --watch
```

Monitors the filesystem for `.rs` file changes and re-runs analysis automatically. Useful during refactoring sessions.

### Shell Completions

```bash
# Generate completions for your shell
rustqual --completions bash > ~/.bash_completion.d/rustqual
rustqual --completions zsh > ~/.zfunc/_rustqual
rustqual --completions fish > ~/.config/fish/completions/rustqual.fish
```

## Using with AI Coding Agents

### Why AI-Generated Code Needs Structural Analysis

AI coding agents (Claude Code, Cursor, Copilot, etc.) are excellent at producing working code quickly, but they consistently exhibit structural problems that rustqual is designed to catch:

- **IOSP violations**: AI agents routinely generate functions that mix orchestration with logic — calling helper functions inside `if` blocks, combining validation with dispatch. These "god-functions" are hard to test and hard to maintain.
- **Complexity creep**: Generated functions tend to be long, deeply nested, and full of inline logic rather than composed from small, focused operations.
- **Duplication**: When asked to implement similar features, AI agents often copy-paste patterns rather than extracting shared abstractions, leading to DRY violations.
- **Weak tests**: AI-generated tests frequently lack meaningful assertions, contain overly long test functions, or rely heavily on mocks without verifying real behavior. The Test Quality dimension catches assertion-free tests, low assertion density, and coverage gaps.

IOSP is particularly valuable for AI-generated code because it enforces a strict decomposition: every function is either an Integration (orchestrates, no logic) or an Operation (logic, no own calls). This constraint forces the kind of small, testable, single-purpose functions that AI agents tend not to produce on their own.

### CLAUDE.md / Cursor Rules Integration

Project-level instruction files (`.claude/CLAUDE.md`, `.cursorrules`, etc.) can teach AI agents to follow IOSP principles. Add rules like these to your project:

```markdown
## Code Quality Rules

- Run `rustqual src/` after making changes. All findings must be resolved.
- Follow IOSP: every function is either an Integration (calls other functions,
  no logic) or an Operation (contains logic, no own-function calls). Never mix both.
- Keep functions under 60 lines and cognitive complexity under 15.
- Do not duplicate logic — extract shared patterns into reusable Operations.
- Do not introduce functions with more than 5 parameters.
- Every test function must contain at least one assertion (assert!, assert_eq!, etc.).
- Generate LCOV coverage data and pass it via `--coverage` to verify coverage gaps.
```

This works with any AI tool that reads project-level instruction files. The key insight is that the agent gets actionable feedback: rustqual tells it exactly which function violated which principle, so it can self-correct.

### CI Quality Gate for AI-Generated Code

Add rustqual to your CI pipeline so that AI-generated PRs are automatically checked:

```yaml
name: Quality Check
on: [pull_request]

jobs:
  quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install rustqual cargo-llvm-cov
      - name: Generate coverage data
        run: cargo llvm-cov --lcov --output-path lcov.info
      - name: Check quality (changed files only)
        run: rustqual --diff HEAD~1 --coverage lcov.info --fail-on-warnings --format github
```

Key flags for AI workflows:
- `--diff HEAD~1` — only analyze files changed in the PR, not the entire codebase
- `--coverage lcov.info` — include test quality coverage analysis (TQ-005)
- `--fail-on-warnings` — treat suppression ratio violations as errors
- `--min-quality-score 90` — reject PRs that drop quality below a threshold
- `--format github` — produces inline annotations on the PR diff

See [CI Integration](#ci-integration) for more workflow examples including baseline comparison.

### Pre-commit Hook

Catch violations before they enter version control — especially useful when AI agents generate code locally:

```bash
#!/bin/bash
# .git/hooks/pre-commit
if ! rustqual src/ 2>/dev/null; then
    echo "rustqual: quality findings detected. Please refactor before committing."
    exit 1
fi
```

This gives the AI agent (or developer) immediate feedback before the code is committed. See [Pre-commit Hook](#pre-commit-hook) for the basic setup.

### Recommended Workflow

The full quality loop for AI-assisted development:

1. **Agent instructions** — CLAUDE.md / Cursor rules teach the agent IOSP principles and rustqual usage
2. **Pre-commit hook** — catches violations locally before they enter version control
3. **Coverage verification** — generate LCOV data with `cargo llvm-cov` and pass via `--coverage` to detect weak or missing tests
4. **CI quality gate** — prevents merges below quality threshold using `--min-quality-score` or `--fail-on-regression`
5. **Baseline tracking** — `--save-baseline` and `--compare` track quality score over time, ensuring AI-generated code does not erode structural quality

## Architecture

The analyzer uses a **two-pass pipeline**:

```
                         ┌──────────────────────────────────┐
                         │          Pass 1: Collect          │
   .rs files ──read──►   │  Read + Parse all files (rayon)   │
                         │  Build ProjectScope (all names)   │
                         │  Scan for // qual:allow markers   │
                         └────────────────┬─────────────────┘
                                          │
                         ┌────────────────▼─────────────────┐
                         │          Pass 2: Analyze          │
                         │  For each function:               │
                         │   BodyVisitor walks AST           │
                         │   → logic + call occurrences      │
                         │   → complexity metrics            │
                         │   → classify: I / O / V / T       │
                         │  Coupling analysis (use-graph)    │
                         │  DRY detection (normalize+hash)   │
                         │  SRP analysis (LCOM4+composite)   │
                         │  Compute quality score            │
                         └────────────────┬─────────────────┘
                                          │
                         ┌────────────────▼─────────────────┐
                         │           Output                  │
                         │  Text / JSON / GitHub / DOT /     │
                         │  SARIF / HTML / Suggestions /     │
                         │  Baseline comparison              │
                         └──────────────────────────────────┘
```

### Source Files

~80 source files in `src/`, ~23,000 lines total (including tests):

```
src/
├── lib.rs             Crate root: CLI, config, quality gates, run() (~710 lines)
├── cli.rs             Clap CLI struct and argument definitions       (~125 lines)
├── main.rs            Thin binary wrapper (rustqual)                (~5 lines)
├── bin/
│   └── cargo-qual/
│       └── main.rs    Thin binary wrapper (cargo qual)              (~5 lines)
├── pipeline/          Analysis orchestration (split into submodules)
│   ├── mod.rs         run_analysis, output_results                  (~750 lines)
│   ├── discovery.rs   File collection, parsing, git diff            (~245 lines)
│   ├── metrics.rs     Coupling + SRP + DRY computation              (~400 lines)
│   └── warnings.rs    Complexity/ext warnings, suppression ratio    (~385 lines)
├── analyzer/
│   ├── mod.rs         Core analysis engine, Analyzer struct         (~908 lines)
│   ├── types.rs       Classification, FunctionAnalysis, metrics     (~290 lines)
│   ├── visitor/       BodyVisitor (AST walking, trivial match)
│   │   ├── mod.rs     Struct, helpers, is_trivial_match_arm         (~630 lines)
│   │   └── visit.rs   Visit trait implementation                    (~290 lines)
│   └── classify.rs    classify_function (3-tuple w/ own_calls)      (~330 lines)
├── config/
│   ├── mod.rs         Config loading, glob compilation              (~475 lines)
│   ├── init.rs        Tailored config generation, ProjectMetrics    (~410 lines)
│   └── sections.rs    Sub-configs, DEFAULT_* constants, WeightsConfig (~380 lines)
├── dry/
│   ├── mod.rs         FileVisitor trait, function collectors        (~600 lines)
│   ├── functions.rs   Duplicate function detection                  (~433 lines)
│   ├── fragments.rs   Fragment-level duplicate detection            (~809 lines)
│   ├── dead_code.rs   Dead code detection                           (~470 lines)
│   ├── wildcards.rs   Wildcard import detection                     (~265 lines)
│   └── boilerplate/   Boilerplate pattern detection (BP-001–010)
│       ├── mod.rs     Types, helpers, detect_boilerplate()          (~140 lines)
│       └── ...        10 per-pattern files (BP-001–BP-010)
├── report/
│   ├── mod.rs         AnalysisResult, Summary, quality score        (~620 lines)
│   ├── text/          Text format output (split into submodules)
│   │   ├── mod.rs     print_report, file/function entries           (~300 lines)
│   │   ├── summary.rs Summary section printers                      (~125 lines)
│   │   ├── dry.rs     DRY section printer                           (~100 lines)
│   │   ├── coupling.rs Coupling section printer                     (~200 lines)
│   │   └── srp.rs     SRP section printer                           (~80 lines)
│   ├── json.rs        JSON format output                            (~450 lines)
│   ├── json_types.rs  Serializable JSON struct definitions          (~200 lines)
│   ├── github.rs      GitHub Actions annotations                    (~375 lines)
│   ├── dot.rs         DOT/Graphviz output                           (~155 lines)
│   ├── sarif/         SARIF v2.1.0 output (split into submodule)
│   │   ├── mod.rs     print_sarif Integration + envelope            (~455 lines)
│   │   └── collectors.rs  collect_*_findings() Operations           (~300 lines)
│   ├── html/          Self-contained HTML report (split into submodules)
│   │   ├── mod.rs     print_html, build_html_string, dashboard      (~240 lines)
│   │   ├── sections.rs IOSP, complexity, coupling sections          (~240 lines)
│   │   └── tables.rs  DRY + SRP sections, generic table builder     (~300 lines)
│   ├── suggestions.rs Refactoring suggestions                       (~192 lines)
│   └── baseline.rs    Baseline v2 save/compare                      (~456 lines)
├── srp/
│   ├── mod.rs         SRP types, visitors, constructor detection    (~535 lines)
│   ├── cohesion.rs    LCOM4 (with constructor support), composite   (~420 lines)
│   └── module.rs      Production lines, function cohesion clusters  (~580 lines)
├── coupling/          Module coupling analysis (split into submodules)
│   ├── mod.rs         Types, analyze_coupling Integration, tests    (~430 lines)
│   ├── graph.rs       build_module_graph (use-tree walking)         (~100 lines)
│   ├── metrics.rs     compute_coupling_metrics (Ca/Ce/I)            (~45 lines)
│   ├── cycles.rs      detect_cycles (Kosaraju iterative SCC)        (~80 lines)
│   └── sdp.rs         Stable Dependencies Principle check           (~210 lines)
├── normalize.rs       AST normalization for DRY                     (~784 lines)
├── findings.rs        Dimension enum, suppression parsing           (~240 lines)
├── scope.rs           ProjectScope, two-pass name resolution        (~264 lines)
└── watch.rs           File watcher for --watch mode                 (~126 lines)
```

### How Classification Works

1. **Trivial check**: Empty bodies are immediately `Trivial`. Single-statement bodies are analyzed — only classified as Trivial if they contain neither logic nor own calls.
2. **AST walking**: `BodyVisitor` implements `syn::visit::Visit` to walk the function body, recording:
   - **Logic**: `if`, `match`, `for`, `while`, `loop`, binary operators (`+`, `&&`, `>`, etc.), optionally `?` operator
   - **Own calls**: function/method calls that match names defined in the project (via `ProjectScope`)
   - **Nesting depth**: tracks control-flow nesting for complexity metrics
3. **Classification**:
   - Logic only → **Operation**
   - Own calls only → **Integration**
   - Both → **Violation** (with severity based on finding count)
   - Neither → **Trivial**
4. **Recursion exception**: If `allow_recursion` is enabled and the only own call is to the function itself, it's classified as Operation instead of Violation.

### ProjectScope: Solving the Method Call Problem

Without type information, the analyzer cannot distinguish `self.push(x)` (Vec method, external) from `self.analyze(x)` (own method). The `ProjectScope` solves this with a two-pass approach:

1. **First pass**: Scan all `.rs` files and collect every declared function, method, struct, enum, and trait name.
2. **Second pass**: During analysis, a call is only counted as "own" if the name exists in the project scope.

This means `v.push(1)` is never counted as own (since `push` is not defined in your project), while `self.analyze_file(f)` is (because `analyze_file` is defined in your project).

**Universal methods** (~26 entries like `new`, `default`, `fmt`, `clone`, `eq`, ...) are always treated as external, even if your project implements them via trait impls. This prevents false positives from standard trait implementations.

### IOSP Score

```
IOSP Score = (Integrations + Operations) / (Integrations + Operations + Violations) × 100%
```

Trivial and suppressed functions are excluded because they are too small or explicitly allowed.

## CI Integration

### GitHub Actions

```yaml
name: Quality Check
on: [push, pull_request]

jobs:
  quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Install rustqual
        run: cargo install --path .
      - name: Check code quality
        run: rustqual src/ --min-quality-score 90 --format github
```

### GitHub Actions with Baseline

```yaml
- name: Check quality regression
  run: |
    rustqual src/ --compare baseline.json --fail-on-regression --format github
```

### Generic CI (JSON)

```yaml
- name: Quality Check
  run: |
    cargo run --release -- src/ --json > quality-report.json
    cat quality-report.json
```

### Pre-commit Hook

```bash
#!/bin/bash
# .git/hooks/pre-commit
if ! cargo run --quiet -- src/ 2>/dev/null; then
    echo "Quality findings detected. Please refactor before committing."
    exit 1
fi
```

## How to Fix Violations

When a function is flagged as a violation, refactor by splitting it into pure integrations and operations:

**Before (violation):**
```rust
fn process(data: &Data) -> Result<Output> {
    if data.value > threshold() {  // logic + call mixed
        transform(data)
    } else {
        default_output()
    }
}
```

**After (IOSP compliant):**
```rust
// Integration: orchestrates, no logic
fn process(data: &Data) -> Result<Output> {
    let threshold = threshold();
    let exceeds = check_threshold(data.value, threshold);
    select_output(exceeds, data)
}

// Operation: logic only, no own calls
fn check_threshold(value: f64, threshold: f64) -> bool {
    value > threshold
}

// Integration: delegates to transform or default
fn select_output(exceeds: bool, data: &Data) -> Result<Output> {
    if exceeds { transform(data) } else { default_output() }
    // Note: this is still a violation! Further refactoring needed:
    // Move the if-logic into an operation, call it from here.
}
```

**Common refactoring patterns:**

| Pattern | Approach |
|---------|----------|
| `if` + call in branch | Extract the condition into an Operation, use `.then()` or pass result to Integration |
| `for` loop with calls | Use iterator chains (`.iter().map(\|x\| process(x)).collect()`) — closures are lenient |
| Match + calls | Extract match logic into an Operation that returns an enum/value, dispatch in Integration |

Use `--suggestions` to get automated refactoring hints.

## Self-Compliance

rustqual **analyzes itself** with zero findings:

```bash
$ cargo run -- src/ --fail-on-warnings

═══ Summary ═══
  Functions: 310    Quality Score: 100.0%

  IOSP:          100.0%  (60I, 153O, 97T)
  Complexity:    100.0%
  DRY:           100.0%
  SRP:           100.0%
  Test Quality:  100.0%
  Coupling:      100.0%

  ~ All allows:   14 (qual:allow + #[allow])

All quality checks passed! ✓
```

This is verified by the integration test suite and CI.

## Testing

```bash
cargo test           # 597 tests (590 unit + 4 integration + 3 showcase)
RUSTFLAGS="-Dwarnings" cargo clippy --all-targets  # lint check (0 warnings)
```

The test suite covers:
- **analyzer/** (tests across 4 modules): classification, closures, iterators, scope integration, recursion, `?` operator, async/await, severity, complexity metrics, suppression
- **config/** (tests across 2 modules): external call matching, ignore patterns, config loading, validation, glob compilation, default generation
- **report/** (tests across 8 modules): summary statistics, JSON structure, suppression counting, baseline roundtrip, complexity, HTML generation, SARIF structure, GitHub annotations
- **dry/** (tests across 5 modules): duplicate detection, fragment detection, dead code detection, boilerplate patterns, normalization
- **srp/** (tests across 3 modules): LCOM4 computation, composite scoring, module line counting, function cohesion clusters (shared-caller unification)
- **pipeline** (25+ tests): file collection, suppression lines, coupling suppression, SRP suppression, suppression ratio
- **scope** (16 tests): scope collection, `is_own_function`, `is_own_method`
- **integration** (4 tests): self-analysis, sample expectations, JSON validity, verbose output
- **showcase** (3 tests): before/after IOSP refactoring examples

## Known Limitations

1. **Syntactic analysis only**: Uses `syn` for AST parsing without type resolution. Cannot determine the receiver type of method calls — relies on `ProjectScope` heuristics and `external_prefixes` config as fallbacks.
2. **Macros**: Macro invocations are not expanded. `println!` etc. are handled as special cases via `external_prefixes`, but custom macros producing logic or calls may be misclassified.
3. **External file modules**: `mod foo;` declarations pointing to separate files are not followed. Only inline modules (`mod foo { ... }`) are analyzed recursively.
4. **Parallelization**: The analysis pass is sequential because `proc_macro2::Span` (with `span-locations` enabled for line numbers) is not `Sync`. File I/O is parallelized via `rayon`.

## Dependencies

| Crate          | Purpose                                        |
|----------------|-------------------------------------------------|
| `syn`          | Rust AST parsing (with `full`, `visit` features)|
| `proc-macro2`  | Span locations for line numbers                 |
| `quote`        | Token stream formatting (generic type display)  |
| `derive_more`  | `Display` derive for analysis types             |
| `clap`         | CLI argument parsing                            |
| `clap_complete`| Shell completion generation                     |
| `walkdir`      | Recursive directory traversal                   |
| `colored`      | Terminal color output                           |
| `serde`        | Config deserialization                          |
| `toml`         | TOML config file parsing                        |
| `serde_json`   | JSON output serialization                       |
| `globset`      | Glob pattern matching for ignore/exclude        |
| `rayon`        | Parallel file I/O                               |
| `notify`       | File system watching for `--watch` mode         |

## License

MIT
