# Reference: CLI flags

```
rustqual [OPTIONS] [PATH]
```

`PATH` defaults to `.`. Run from the project root so architecture globs (`src/**`) match.

## Output and verbosity

| Flag | Description |
|---|---|
| `-v`, `--verbose` | Show every function with metrics, not just findings. |
| `--findings` | One finding per line: `file:line category detail in fn_name`. Useful for piping. |
| `--format <FMT>` | Output format. One of: `text` (default), `json`, `github`, `dot`, `sarif`, `html`, `ai`, `ai-json`. See [reference-output-formats.md](./reference-output-formats.md). |
| `--json` | Shortcut for `--format json`. |
| `--suggestions` | Show refactoring suggestions for IOSP violations. |

## Analysis behaviour

| Flag | Description |
|---|---|
| `-c`, `--config <FILE>` | Path to config. Default: `rustqual.toml` in the target directory. |
| `--diff [REF]` | Only analyse files changed vs a git ref (default: `HEAD`). Conflicts with `--watch`. |
| `--coverage <LCOV>` | Path to LCOV coverage file. Enables TQ-004 / TQ-005. |
| `--explain <FILE>` | Diagnostic mode: explain architecture-rule classification for one file. |
| `--watch` | Watch for file changes and re-analyse continuously. |

## Strictness toggles

| Flag | Description |
|---|---|
| `--strict-closures` | Treat closures as logic (stricter IOSP). |
| `--strict-iterators` | Treat iterator chains (`.map`, `.filter`, …) as logic. |
| `--strict-error-propagation` | Count `?` as logic (implicit control flow). |
| `--allow-recursion` | Allow recursive calls — don't count as violations. |

## Exit-code controls

| Flag | Description |
|---|---|
| `--no-fail` | Don't exit `1` on findings. Useful for local exploration. |
| `--fail-on-warnings` | Treat warnings (suppression-ratio overrun, etc.) as errors. |
| `--min-quality-score <N>` | Minimum overall quality score (0–100). Exit `1` if below. |
| `--fail-on-regression` | Used with `--compare`. Exit `1` only when quality regresses vs baseline. |

## Baseline / regression

| Flag | Description |
|---|---|
| `--save-baseline <FILE>` | Save current results as baseline JSON. |
| `--compare <FILE>` | Compare current results against a saved baseline. |

Typical workflow:

```bash
rustqual --save-baseline baseline.json
git add baseline.json && git commit -m "Add quality baseline"

# In CI:
rustqual --compare baseline.json --fail-on-regression
```

## Sorting

| Flag | Description |
|---|---|
| `--sort-by-effort` | Sort IOSP violations by refactoring effort (highest first). |

## Project setup

| Flag | Description |
|---|---|
| `--init` | Generate a `rustqual.toml` calibrated to your current codebase metrics. |
| `--completions <SHELL>` | Emit shell completions. Supported: `bash`, `zsh`, `fish`, `elvish`, `powershell`. |

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success — no findings, or `--no-fail` set. |
| `1` | Findings; or regression vs baseline; or score below `--min-quality-score`; or warnings present (`--fail-on-warnings`). |
| `2` | Configuration error — invalid or unreadable `rustqual.toml`. |

## Common compositions

```bash
# Local exploration
rustqual --no-fail --verbose

# CI hard gate with coverage and PR annotations
rustqual --coverage lcov.info --min-quality-score 90 --fail-on-warnings --format github

# PR-only analysis
rustqual --diff origin/main --format github

# Baseline-based regression gate
rustqual --compare baseline.json --fail-on-regression --format github

# Explain why a file is failing the architecture dimension
rustqual --explain src/foo/bar.rs
```

## Related

- [reference-configuration.md](./reference-configuration.md) — every config option in `rustqual.toml`
- [reference-output-formats.md](./reference-output-formats.md) — every `--format` value with examples
- [ci-integration.md](./ci-integration.md) — putting flags together in CI
