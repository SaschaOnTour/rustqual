# Use case: CI/CD integration

Run rustqual on every push or pull request. The defaults make this easy — the binary already exits with code `1` on any finding, so a single `run:` line in CI is enough for a hard quality gate.

## GitHub Actions — minimal

```yaml
name: Quality Check
on: [push, pull_request]

jobs:
  quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install rustqual
      - run: rustqual
```

That's all. If any dimension flags a finding, the job fails.

## GitHub Actions — with inline PR annotations

```yaml
- run: rustqual --format github --min-quality-score 90
```

`--format github` emits `::error::` and `::warning::` annotations that GitHub renders inline on the PR diff. `--min-quality-score 90` enforces a hard floor on the overall quality score.

## GitHub Actions — changed files only

For large codebases, restrict analysis to files changed in the PR:

```yaml
- run: rustqual --diff origin/main --format github
```

`--diff <REF>` runs the same analysis but only reports findings in files that differ from `<REF>`. Cuts CI time for large codebases without losing PR-level signal.

## GitHub Actions — with coverage

The Test Quality dimension can use LCOV coverage data to detect untested logic:

```yaml
- run: cargo install rustqual cargo-llvm-cov
- run: cargo llvm-cov --lcov --output-path lcov.info
- run: rustqual --coverage lcov.info --format github
```

This enables TQ-005 (uncovered logic detection) on top of the static checks (assertion-free tests, no-SUT tests, untested public functions).

## GitHub Actions — baseline comparison

For codebases that aren't yet at 100% but want to prevent regression:

```yaml
- run: rustqual --compare baseline.json --fail-on-regression --format github
```

The job fails only when the quality score drops or new findings appear. New code can't make things worse, but you don't have to fix everything before the next merge.

Generate the baseline once and commit it:

```bash
rustqual --save-baseline baseline.json
git add baseline.json && git commit -m "Add quality baseline"
```

Update it intentionally as part of refactor PRs.

## SARIF for GitHub Code Scanning

```yaml
- run: rustqual --format sarif > rustqual.sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: rustqual.sarif
```

Findings appear in the **Security** tab as Code Scanning alerts, with rule IDs for every dimension (IOSP, complexity, coupling, DRY, SRP, test quality, architecture).

## Other CI systems

rustqual has no GitHub-specific dependencies — it's a regular Rust binary. For GitLab, CircleCI, Jenkins, etc., the only difference is which output format you want:

- `--format json` — pipe to whatever tool you have
- `--format html` — self-contained HTML report you can publish as an artifact
- Default text output — printed to stdout

Example GitLab snippet:

```yaml
quality:
  image: rust:latest
  script:
    - cargo install rustqual
    - rustqual --format json > quality.json
  artifacts:
    paths:
      - quality.json
```

## Pre-commit hook

For local enforcement before code reaches CI:

```bash
#!/bin/bash
# .git/hooks/pre-commit
if ! rustqual 2>/dev/null; then
    echo "rustqual: quality findings detected. Refactor before committing."
    exit 1
fi
```

`chmod +x .git/hooks/pre-commit` to enable.

## Quality gates in practice

The flags compose. A typical "production-grade" CI step:

```yaml
- run: |
    rustqual \
      --diff origin/main \
      --coverage lcov.info \
      --min-quality-score 90 \
      --fail-on-warnings \
      --format github
```

This:
- analyses only files changed vs `main`,
- includes coverage-based test-quality checks,
- requires the overall quality score to be at least 90%,
- treats suppression-ratio overruns as hard errors,
- emits inline PR annotations.

`--fail-on-warnings` is worth knowing: by default, exceeding `max_suppression_ratio` (5% of functions suppressed) emits a warning but doesn't fail. With this flag, it does.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success — no findings, or `--no-fail` set |
| `1` | Quality findings; or regression vs baseline (`--fail-on-regression`); or score below threshold (`--min-quality-score`); or warnings present (`--fail-on-warnings`) |
| `2` | Configuration error — invalid or unreadable `rustqual.toml` |

Full flag reference: [reference-cli.md](./reference-cli.md).

## Related

- [ai-coding-workflow.md](./ai-coding-workflow.md) — patterns specific to AI-generated code
- [legacy-adoption.md](./legacy-adoption.md) — onboarding rustqual on existing codebases
- [reference-output-formats.md](./reference-output-formats.md) — every format with examples
