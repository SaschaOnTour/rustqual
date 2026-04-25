# rustqual

[![CI](https://github.com/SaschaOnTour/rustqual/actions/workflows/ci.yml/badge.svg)](https://github.com/SaschaOnTour/rustqual/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rustqual.svg)](https://crates.io/crates/rustqual)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

Comprehensive Rust code quality analyzer — six dimensions: Complexity, Coupling, DRY, IOSP, SRP, Test Quality — plus 7 structural binary checks integrated into SRP and Coupling. Particularly useful as a structural quality guardrail for AI-generated code, catching the god-functions, mixed concerns, duplicated patterns, and weak tests that AI coding agents commonly produce.

## Quality Dimensions

rustqual analyzes your Rust code across **seven quality dimensions**, each contributing to an overall quality score:

| Dimension    | Weight | What it checks |
|--------------|--------|----------------|
| IOSP         | 22%    | Function separation (Integration vs Operation) |
| Complexity   | 18%    | Cognitive/cyclomatic complexity, magic numbers, nesting depth, function length, unsafe blocks, error handling |
| DRY          | 13%    | Duplicate functions, fragments, dead code, boilerplate |
| SRP          | 18%    | Struct cohesion (LCOM4), module length, function clusters, structural checks (BTC, SLM, NMS) |
| Coupling     | 9%     | Module instability, circular dependencies, SDP, structural checks (OI, SIT, DEH, IET) |
| Test Quality | 10%    | Assertion density, no-SUT tests, untested functions, coverage gaps, untested logic |
| Architecture | 10%    | Layer ordering, forbidden-edge rules, symbol patterns (path/method/function/macro/derive/item-kind), trait-signature contracts |

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
rustqual                 # direct invocation (defaults to .)
cargo qual               # as cargo subcommand
```

## Quick Start

```bash
# Analyze current directory (default — matches architecture-rule globs)
rustqual

# Analyze a specific file or directory
rustqual src/lib.rs

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
| `--explain <FILE>`              | Architecture dimension: print layer assignment, classified imports, and active rules for one file |

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
# Ignore Functions
# ────────────────────────────────────────────────────────────────
# Functions matching these patterns are completely excluded from analysis.
# Supports full glob syntax: *, ?, [abc], [!abc]
ignore_functions = [
    "main",      # entry point, always mixes logic + calls
    "run",       # composition-root dispatcher
    "visit_*",   # syn::Visit trait implementations (external dispatch)
]

# ────────────────────────────────────────────────────────────────
# Exclude Files
# ────────────────────────────────────────────────────────────────
# Glob patterns for files to exclude from analysis entirely.
exclude_files = ["examples/**"]      # e.g. fixture crates for rule demos

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
detect_repeated_matches = true       # Flag repeated match blocks (DRY-005)

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
weights = [0.4, 0.25, 0.15, 0.2]     # [lcom4, fields, methods, fan_out]
file_length_baseline = 300           # Production lines before penalty starts
file_length_ceiling = 800            # Production lines at maximum penalty
max_independent_clusters = 2         # Highest allowed (warn on 3+ clusters)
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
[test_quality]
enabled = true
coverage_file = ""                   # Path to LCOV file (or use --coverage CLI flag)
# Extra macro names (beyond assert*/debug_assert*) to recognize as assertions in TQ-001
# extra_assertion_macros = ["verify", "check", "expect_that"]

# ────────────────────────────────────────────────────────────────
# Quality Weights
# ────────────────────────────────────────────────────────────────
[weights]
iosp         = 0.22
complexity   = 0.18
dry          = 0.13
srp          = 0.18
coupling     = 0.09
test_quality = 0.10
architecture = 0.10
# Weights must sum to 1.0

# ────────────────────────────────────────────────────────────────
# Architecture Dimension (see "Architecture Dimension" section for details)
# ────────────────────────────────────────────────────────────────
[architecture]
enabled = true

[architecture.layers]
order = ["domain", "port", "infrastructure", "analysis", "application"]
unmatched_behavior = "strict_error"  # or "composition_root"

[architecture.layers.domain]
paths = ["src/domain/**"]

[architecture.layers.port]
paths = ["src/ports/**"]

[architecture.layers.infrastructure]
paths = [
    "src/adapters/config/**",
    "src/adapters/source/**",
    "src/adapters/suppression/**",
]

[architecture.layers.analysis]
paths = [
    "src/adapters/analyzers/**",
    "src/adapters/shared/**",
    "src/adapters/report/**",
]

[architecture.layers.application]
paths = ["src/app/**"]

[architecture.reexport_points]
paths = [
    "src/lib.rs",
    "src/main.rs",
    "src/adapters/mod.rs",
    "src/bin/**",
    "src/cli/**",
    "tests/**",
]

# Optional: map external crate names to your own layers (for workspaces)
[architecture.external_crates]
# "my_domain_crate" = "domain"
# "my_infra_crate"  = "infrastructure"

# Forbidden edges (cross-branch imports the layer rule permits but you don't want)
[[architecture.forbidden]]
from = "src/adapters/analyzers/**"
to = "src/adapters/report/**"
reason = "Analyzers produce findings; reporters consume them separately"

# Symbol patterns (see "Architecture Dimension" below for all 7 matcher types)
[[architecture.pattern]]
name = "no_panic_helpers_in_production"
forbid_method_call = ["unwrap", "expect"]
forbidden_in = ["src/**"]
except = ["**/tests/**"]
reason = "Production propagates errors through Result"

[[architecture.pattern]]
name = "no_syn_in_domain"
forbid_path_prefix = ["syn::", "proc_macro2::", "quote::"]
forbidden_in = ["src/domain/**"]
reason = "Domain types know no AST representation"

# Trait-signature rule (port contract)
[[architecture.trait_contract]]
name = "port_traits"
scope = "src/ports/**"
receiver_may_be = ["shared_ref"]
forbidden_return_type_contains = ["anyhow::", "Box<dyn"]
forbidden_error_variant_contains = ["syn::", "toml::", "serde_json::"]
must_be_object_safe = true
required_supertraits_contain = ["Send", "Sync"]

# ────────────────────────────────────────────────────────────────
# Report Aggregation
# ────────────────────────────────────────────────────────────────
[report]
aggregation = "loc_weighted"         # workspace-mode score aggregation
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

**Multi-line rationales are supported.** If you want to explain *why* a suppression is in place over several comment lines, just put them directly below the marker — the annotation window measures from the block's last comment line, not from the marker itself. Works with `#[derive]` in between:

```rust
// qual:allow(srp) — false-positive LCOM4=2
// The struct's methods form one coherent data-layer abstraction
// (validate() reads every field; append() calls it via debug_assert!).
#[derive(Default)]
pub struct LayerStorage { /* ... */ }
```

A blank line breaks the block — misplaced markers (marker far away from the item with a gap) don't silently reach across.

**Orphan detection.** Any `// qual:allow(...)` marker that doesn't match a finding in its window is emitted as an `ORPHAN_SUPPRESSION` finding in every output format (text, JSON, AI, SARIF). Typical causes:

- *Stale*: the underlying finding was fixed; the marker was left behind.
- *Misplaced*: the marker is too far from the item (outside `ANNOTATION_WINDOW=3` after block-end shift).
- *Wrong dimension*: the marker says `qual:allow(dry)` but the real finding at that line is, say, SRP.

Orphans appear in `--findings` output and count toward `total_findings()`, so default-fail (`Err(1)` on any finding) triggers on them — a one-shot rustqual run surfaces every stale marker for cleanup. They do not currently gate `--fail-on-warnings` (which only checks `suppression_ratio_exceeded`). `// qual:allow(coupling)` markers are exempt from orphan detection because coupling warnings are module-global (no file/line anchor to match).

### API Annotation

Mark public API functions with `// qual:api` to exclude them from dead code (DRY-002) and untested function (TQ-003) detection:

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

### Test-Helper Annotation

Mark integration-test helpers with `// qual:test_helper` to exclude them from dead code (DRY-002 `testonly`) and untested function (TQ-003) detection, **while keeping every other check active**:

```rust
// qual:test_helper
pub fn assert_in_range(actual: f64, expected: f64, tolerance: f64) {
    assert!((actual - expected).abs() < tolerance);
}
```

This is the narrow fix for the „helper called from `tests/*.rs` but not from production" case that used to force a choice between `ignore_functions` (which silently disables **every** check for that function) and a `qual:allow(dry)` + `qual:allow(test_quality)` stack (which costs against the suppression ratio). Semantic distinction from `qual:api`:

| Marker | Intent | What it suppresses |
|---|---|---|
| `// qual:api` | „this is the public library API" | DRY-002 (`testonly` dead code) + TQ-003 (untested) |
| `// qual:test_helper` | „this exists so test binaries can call into it" | DRY-002 `testonly` dead code + TQ-003 (untested) |

Neither marker counts against `max_suppression_ratio`. Complexity, SRP, duplicate detection, and coupling checks keep applying — if a test helper grows to 200 lines with nested match arms, `LONG_FN` and `COGNITIVE` will still flag it.

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

More broadly, calls to **any non-Violation function** are treated as safe — this includes Operations (pure logic), Trivials (empty/simple), and Integrations (pure delegation). Only calls to other Violations (functions that themselves mix logic and non-safe calls) remain true Violations. This cascades iteratively until stable.

> **Design note — pragmatic IOSP relaxation:** In strict IOSP, *any* call to an own function from a function with logic constitutes a Violation. rustqual relaxes this: only calls to Violations count as concern-mixing. Calls to well-structured functions (Operations, Integrations, Trivials) are treated as safe building blocks. This eliminates false positives for common patterns while preserving true Violations where tangled code calls other tangled code (e.g., mutually recursive Violations).

### Recursive Annotation

Mark intentionally recursive functions with `// qual:recursive` to prevent the self-call from being counted as an own call:

```rust
// qual:recursive
fn traverse(node: &Node) -> Vec<String> {
    let mut result = vec![node.name.clone()];
    for child in &node.children {
        result.extend(traverse(child));  // self-call not counted
    }
    result
}
```

Without the annotation, `traverse` would be a Violation (loop logic + self-call). With it, the self-call is removed before classification. Like `// qual:api` and `// qual:inverse`, recursive markers do **not** count against the suppression ratio.

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

The overall quality score is a weighted average of seven dimension scores (weights are configurable via `[weights]` in `rustqual.toml`):

| Dimension    | Default Weight | Metric |
|--------------|----------------|--------|
| IOSP         | 22%            | Compliance ratio (non-trivial functions) |
| Complexity   | 18%            | 1 - (complexity + magic numbers + nesting + length + unsafe + error handling) / total |
| DRY          | 13%            | 1 - (duplicates + fragments + dead code + boilerplate + wildcards + repeated matches) / total |
| SRP          | 18%            | 1 - (struct warnings + module warnings + param warnings + structural BTC/SLM/NMS) / total |
| Coupling     | 9%             | 1 - (coupling warnings + 2×cycles + SDP violations + structural OI/SIT/DEH/IET) / total |
| Test Quality | 10%            | 1 - (assertion-free + no-SUT + untested + uncovered + untested-logic) / total |
| Architecture | 10%            | 1 - (layer violations + forbidden edges + pattern hits + trait-contract breaches) / total |

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
- **Module-level (cohesion)**: Detects files with too many independent function clusters. Uses Union-Find on private substantive functions, leveraging IOSP own-call data. Functions that call each other or share a common caller are united into the same cluster. A file with more than `max_independent_clusters` (default 2, so 3+ clusters trigger) independent groups indicates multiple responsibilities that should be split into separate modules.

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

### Architecture Dimension (v1.0)

Four rule types check the structural shape of the codebase against an
explicit layered architecture. Enabled via `[architecture] enabled = true`.

**Layer Rule** — files are assigned to layers via path globs; inner layers
(lower rank) may not import from outer layers (higher rank). With
`unmatched_behavior = "strict_error"`, every production file must match
a layer glob; unmatched files become violations. With `"composition_root"`,
unmatched files bypass the rule entirely (useful for Cargo-workspace roots).

A minimal hexagonal layering:

```toml
[architecture.layers]
order = ["domain", "port", "application", "adapter"]
unmatched_behavior = "strict_error"

[architecture.layers.domain]
paths = ["src/domain/**"]

[architecture.layers.port]
paths = ["src/ports/**"]

[architecture.layers.application]
paths = ["src/app/**"]

[architecture.layers.adapter]
paths = ["src/adapters/**"]

[architecture.reexport_points]
paths = ["src/lib.rs", "src/main.rs"]
```

A file in `src/domain/**` importing from `src/adapters/**` is flagged.
Rustqual itself uses a five-rank variant that separates
infrastructure-style adapters (`config`, `source`, `suppression`) from
analysis-logic adapters (`analyzers`, `shared`, `report`) — see the
committed `rustqual.toml` for the full structure.

**Forbidden Rule** — paired `from` / `to` path globs forbid cross-branch
imports:

```toml
[[architecture.forbidden]]
from = "src/adapters/analyzers/iosp/**"
to = "src/adapters/analyzers/**"
except = ["src/adapters/analyzers/iosp/**"]
reason = "peer analyzers are isolated"
```

**Symbol Patterns** — ban specific language shapes via seven matchers:

| Matcher | Hits |
|---------|------|
| `forbid_path_prefix` | any path reference starting with a banned prefix |
| `forbid_glob_import` | `use foo::*;` |
| `forbid_method_call` | `x.unwrap()` / UFCS `Option::unwrap(x)` |
| `forbid_function_call` | `Box::new(…)` via fully-qualified path |
| `forbid_macro_call` | `panic!()`, `println!()`, etc. |
| `forbid_item_kind` | `async_fn`, `unsafe_fn`, `unsafe_impl`, `static_mut`, `extern_c_block`, `inline_cfg_test_module`, `top_level_cfg_test_item` |
| `forbid_derive` | `#[derive(Serialize)]` |

Scope is XOR: either `allowed_in` (whitelist) or `forbidden_in` (blocklist),
with `except` as fine-grained overrides. Example:

```toml
[[architecture.pattern]]
name = "no_panic_in_production"
forbid_macro_call = ["panic", "todo", "unreachable"]
forbidden_in = ["src/**"]
except = ["**/tests/**"]
reason = "production code returns typed errors"
```

**Trait-Signature Rule** — structural checks on trait definitions in scope:

```toml
[[architecture.trait_contract]]
name = "port_traits"
scope = "src/ports/**"
receiver_may_be = ["shared_ref"]
methods_must_be_async = true
forbidden_return_type_contains = ["anyhow::", "Box<dyn"]
required_supertraits_contain = ["Send", "Sync"]
must_be_object_safe = true
```

Checks: `receiver_may_be`, `methods_must_be_async`,
`forbidden_return_type_contains`, `required_param_type_contains`,
`required_supertraits_contain`, `must_be_object_safe` (conservative: flags
`Self` returns and method-level generics), `forbidden_error_variant_contains`.

**5. `[architecture.call_parity]` — cross-adapter delegation drift check (v1.1, hardened in v1.2).**
Detects when N peer adapters (CLI + MCP + REST + …) fall out of sync
with the shared Application layer. Two rules run under one config
section:

- **`no_delegation`** — each `pub fn` in an adapter layer must
  transitively call into the target layer within `call_depth` hops.
  Catches adapter handlers that inline business logic instead of
  delegating to the shared dispatcher.
- **`missing_adapter`** — each `pub fn` in the target layer must be
  reached from every adapter layer. Catches asymmetric feature coverage
  (e.g. CLI + MCP call `application::do_thing`, REST doesn't).

```toml
[architecture.call_parity]
adapters = ["cli", "mcp", "rest"]   # layer names from [architecture.layers]
target   = "application"
call_depth = 3                       # transitive BFS depth (default 3)
# exclude_targets matches on the canonical MODULE path (the crate::
# path with `crate::` stripped), NOT on the layer name. If layer
# `application` is mapped to `src/app/**`, the pattern would be
# `app::setup::*`, not `application::setup::*`.
exclude_targets = ["app::setup::*"]
```

Zero per-function annotation: adapter fns are enumerated automatically
from the layer globs you already have. Shallow type-inference resolves
Session/Service/Context-pattern idioms out of the box:

- **Method-chain constructors:** `let s = Session::open().map_err(f)?;
  s.diff(...)` — the inference walks through `?`, `.unwrap()`, `.expect()`,
  `.map_err()`, `.or_else()`, `.unwrap_or*()` and back to the
  constructor to find `Session`.
- **Field access:** `ctx.session.diff(...)` — looks up `session` in the
  workspace struct-field index, then resolves `diff` on the resulting type.
- **Free-fn return types:** `make_session().unwrap().diff()` — the
  free-fn's declared return type is indexed and flows through the chain.
- **Result/Option combinators:** full stdlib table for `unwrap`,
  `expect`, `ok`, `err`, `map_err`, `or_else`, `ok_or`, `filter`,
  `as_ref` etc. Closure-dependent combinators (`map`, `and_then`)
  intentionally stay unresolved rather than fabricate an edge.
- **Wrapper stripping:** `Arc<T>`, `Box<T>`, `Rc<T>`, `Cow<'_, T>`,
  `&T`, `&mut T` — the Deref-transparent smart pointers — strip to the
  inner type. `RwLock<T>` / `Mutex<T>` / `RefCell<T>` / `Cell<T>` do
  **not** strip by default (their `read` / `lock` / `borrow` / `get`
  methods don't exist on the inner type — stripping would synthesize
  bogus edges). Opt in per-wrapper via `transparent_wrappers` if your
  codebase uses a genuinely Deref-transparent domain wrapper. `Vec<T>`
  / `HashMap<_, V>` preserve the element/value type.
- **`Self::xxx`** in impl-method contexts substitutes to the enclosing
  type.
- **`if let Some(s) = opt`** binds `s: T` when `opt: Option<T>`, same
  for `Ok(x)` / `Err(e)` patterns.
- **Trait dispatch** (`dyn Trait` / `&dyn Trait` / `Box<dyn Trait>`
  receivers): fans out to every workspace impl of the trait. Method
  must be declared on the trait — unrelated methods stay unresolved
  rather than fabricating edges. Marker traits (`Send`, `Sync`, …)
  skipped when picking the dispatch-relevant bound.
- **Turbofish return types**: `get::<Session>()` for generic fns — the
  turbofish arg is used as the return type when the workspace index has
  no concrete return for `get`. Only single-ident paths trigger.
- **Type aliases**: `type Repo = Arc<Box<Store>>;` is recorded and
  expanded during receiver resolution, so `fn h(r: Repo) { r.insert(..) }`
  reaches `Store::insert` through the peeled smart-pointer chain.
  Aliases wrapping non-Deref types (`type Db = Arc<RwLock<Store>>`) still
  expand, but the `RwLock` stops peeling — methods on the inner `Store`
  aren't reached unless `RwLock` is listed in `transparent_wrappers`.

For framework codebases you can extend the wrapper and macro lists:

```toml
[architecture.call_parity]
# Framework extractor wrappers peeled like Arc / Box:
transparent_wrappers = ["State", "Extension", "Json", "Data"]
# Attribute macros that don't affect the call graph. The set is
# recorded for future macro-expansion integrations and currently has
# no observable effect on the call-graph / type-inference pipeline.
transparent_macros = ["my_custom_attr"]
```

Two escape mechanisms:
- `exclude_targets` — glob list in config for whole groups of
  legitimately asymmetric target fns.
- `// qual:allow(architecture)` — per-fn escape for individual
  exceptions. Counts against `max_suppression_ratio`.

See `examples/architecture/call_parity/` for a runnable 3-adapter
fixture.

Known limits (documented, with clear workarounds):
- **Closure-body arg types** `Session::open().map(|r| r.m())` — the
  closure arg's type isn't inferred. Inner method call stays
  `<method>:m`. Workaround: pull the method call out of the closure.
- **Unannotated generics** `let x = get(); x.m()` where `get<T>() -> T`
  — use turbofish `get::<T>()` or `let x: T = get();`.
- **`impl Trait` inherent methods** — `fn make() -> impl Handler; make().trait_method()` resolves to every workspace impl of `Handler::trait_method` via over-approximation, but an inherent method not declared on `Handler` can't be reached (the concrete type is hidden by design).
- **Multi-bound `impl Trait` / `dyn Trait` returns** — `fn make() -> impl Future<Output = T> + Handler` keeps only the first non-marker bound, so `.await` propagation *or* trait-dispatch fires, never both. Marker traits (`Send`/`Sync`/`Unpin`/`Copy`/`Clone`/`Sized`/`Debug`/`Display`) are filtered first, so `impl Future<Output = T> + Send` is unaffected. Workaround: split the return into two methods, or `qual:allow(architecture)` on the call-site.
- **Caller-side `pub use` path-following.** `pub mod outer { mod private { pub struct Hidden; impl Hidden { pub fn op() } } pub use self::private::Hidden; }` with a caller `fn h(x: outer::Hidden) { x.op() }` resolves the parameter to `crate::…::outer::Hidden` while the impl is keyed under `crate::…::outer::private::Hidden`. Visibility is recognised on both paths, but the call-graph edge goes to `<method>:op` because the resolver doesn't follow workspace-wide `pub use` re-exports inside nested modules. Workaround: write `impl outer::Hidden { … }` at the file-level qualified path so impl-canonical and caller-canonical agree, or `qual:allow(architecture)` at the call-site.
- **Re-exported type aliases inside private modules.** `mod private { pub type Public = Hidden; … } pub use private::Public;` doesn't follow into the alias's target — private modules aren't walked by the visibility pass, so the alias's source type stays out of `visible_canonicals`. Workaround: lift the type alias to the parent module (`pub use private::Hidden; pub type Public = Hidden;`) so both the alias declaration and its target are processed.
- **Type-vs-value namespace ambiguity in `pub use`.** A `pub use internal::helper as Hidden;` re-export adds `Hidden` as a workspace-visible *type* canonical without checking whether the leaf is actually a type. If the same scope has a private `struct Hidden`, its impl methods get registered as adapter surface even though the `pub use` only exported a function. Workaround: rename to avoid the value/type collision, or `qual:allow(architecture)` on the affected impl.
- **`impl Alias { … }` with caller-side alias expansion.** `pub type Public = private::Hidden; impl Public { pub fn op(&self) {} }` indexes the method under `crate::…::Public::op` (impl self-type goes through the path canonicaliser), while a caller `fn h(x: Public) { x.op() }` resolves `x` via type-alias expansion to `crate::…::private::Hidden` and produces a `Hidden::op` edge. Visibility recognises `Public`, but the call-graph edges and the indexed method canonical disagree, so Check B reports `Public::op` as unreached. Workaround: declare the `impl` against the source type (`impl private::Hidden { … }`) so impl-canonical and caller-canonical agree, or `qual:allow(architecture)` on the affected impl.
- **Arbitrary proc-macros** not listed in `transparent_macros` —
  `// qual:allow(architecture)` on the enclosing fn is the escape.

Design reference: `docs/rustqual-design-receiver-type-inference.md`.

**`--explain <FILE>`** diagnostic mode prints the file's layer assignment,
classified imports, and rule hits — useful for understanding why a rule
fires or when tuning config:

```
$ cargo run -- --explain src/domain/foo.rs
═══ Architecture Explain: src/domain/foo.rs ═══
Layer: domain (rank 0)

Imports (1):
  line 1: crate::adapters::Foo — crate::adapters → layer adapter

Layer violations:
  line 1: domain ↛ adapter  via crate::adapters::Foo
```

See `examples/architecture/` for a runnable mini-fixture per matcher/rule.
Suppress with `// qual:allow(architecture)` on the file.

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

~200 production files in `src/`, ~19 400 lines. Layered per Clean Architecture:

```
src/
├── lib.rs                      Composition root (run() entry, ~140 lines)
├── main.rs                     Thin binary wrapper (rustqual)
├── bin/cargo-qual/main.rs      Thin binary wrapper (cargo qual)
│
├── domain/                     Pure value types (no syn, no I/O)
│   ├── dimension.rs
│   ├── finding.rs              Port-emitted Finding struct
│   ├── score.rs                PERCENTAGE_MULTIPLIER
│   ├── severity.rs
│   ├── source_unit.rs
│   └── suppression.rs
│
├── ports/                      Trait contracts
│   ├── dimension_analyzer.rs   DimensionAnalyzer + AnalysisContext + ParsedFile
│   ├── reporter.rs
│   ├── source_loader.rs
│   └── suppression_parser.rs
│
├── adapters/
│   ├── config/                 TOML loading, tailored --init, weight validation
│   ├── source/                 Filesystem walk, parse, --watch
│   ├── suppression/            qual:allow marker parsing
│   ├── shared/                 Cross-analyzer utilities
│   │   ├── cfg_test.rs         has_cfg_test, has_test_attr
│   │   ├── cfg_test_files.rs   collect_cfg_test_file_paths
│   │   ├── normalize.rs        AST normalization for DRY
│   │   └── use_tree.rs         Canonical use-tree walker
│   ├── analyzers/              Seven dimension analyzers
│   │   ├── iosp/               Analyzer, BodyVisitor, classify, scope
│   │   ├── complexity/
│   │   ├── dry/                Incl. boilerplate/ (BP-001–BP-010)
│   │   ├── srp/
│   │   ├── coupling/
│   │   ├── tq/
│   │   ├── structural/         BTC, SLM, NMS, OI, SIT, DEH, IET
│   │   └── architecture/       Layer + Forbidden + Symbol + Trait-contract rules
│   └── report/                 Text, JSON, SARIF, HTML, DOT, GitHub,
│                               AI, AI-JSON, baseline, suggestions
│
├── app/                        Application use cases
│   ├── analyze_codebase.rs     Port-based use case
│   ├── pipeline.rs             Full-pipeline orchestrator
│   ├── secondary.rs            Per-dimension secondary passes
│   ├── metrics.rs              Coupling/DRY/SRP helpers
│   ├── tq_metrics.rs
│   ├── structural_metrics.rs
│   ├── architecture.rs         Architecture dim wiring via port
│   ├── warnings.rs             Complexity + leaf reclass + suppression ratio
│   ├── dry_suppressions.rs
│   ├── exit_gates.rs           Default-fail, min-quality, fail-on-warnings
│   └── setup.rs                Config loading + CLI overrides
│
└── cli/
    ├── mod.rs                  Cli struct (clap), OutputFormat
    ├── handlers.rs             --init, --completions, --save-baseline, --compare
    └── explain.rs              --explain <file> architecture diagnostic

tests/                          Workspace integration tests
├── integration.rs              End-to-end CLI invocations
└── showcase_iosp.rs            Before/after IOSP refactor demonstration
```

Companion test trees live next to the production code they cover
(`src/<module>/tests/<name>.rs`). Workspace-root `tests/**` are Cargo's
integration-test binaries; each is its own crate.

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

rustqual **analyzes itself** with zero findings across all seven dimensions:

```bash
$ cargo run -- . --fail-on-warnings --coverage coverage.lcov

═══ Summary ═══
  Functions: 1805    Quality Score: 100.0%

  IOSP:        100.0%  (996I, 270O, 521T)
  Complexity:  100.0%
  DRY:         100.0%
  SRP:         100.0%
  Coupling:    100.0%
  Test Quality:100.0%
  Architecture:100.0%

  ~ All allows:   27 (qual:allow + #[allow])

All quality checks passed! ✓
```

This is verified by the integration test suite and CI. Note: use `.` as
the analysis root (not `src/`) so that architecture-rule globs like
`src/adapters/**` match the actual paths.

## Testing

```bash
cargo nextest run                                    # 1114 tests (1107 unit + 4 integration + 3 showcase)
RUSTFLAGS="-Dwarnings" cargo clippy --all-targets    # lint check (0 warnings)
```

The test suite covers:
- **adapters/analyzers/** — classification, closures, iterators, scope, recursion, `?` operator, async/await, severity, complexity, IOSP/DRY/SRP/coupling/TQ/structural/architecture rule behaviour
- **adapters/config/** — ignore patterns, glob compilation, TOML loading, validation, tailored `--init` generation, weight sum check
- **adapters/report/** — summary stats, JSON structure, suppression counting, baseline roundtrip, HTML, SARIF, GitHub annotations, AI/TOON output
- **adapters/shared/** — cfg-test detection, use-tree walking, AST normalization
- **adapters/source/** — filesystem walk, `--watch` loop
- **app/** — pipeline orchestration, exit gates, setup, secondary-pass coordination, warning accumulation
- **domain/** + **ports/** — value-type invariants and trait-contract shape
- **Integration tests** (`tests/integration.rs`): self-analysis, sample expectations, JSON validity, verbose output
- **Showcase tests** (`tests/showcase_iosp.rs`): before/after IOSP refactoring examples

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
