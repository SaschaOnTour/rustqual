# Reference: output formats

`--format <FMT>` switches the output format. All formats render the same underlying analysis — they differ only in serialisation.

| Format | Use case |
|---|---|
| `text` (default) | Local exploration, terminal use. Coloured summary. |
| `json` | Machine-readable, full detail. Pipe to `jq`, custom dashboards. |
| `github` | `::error::` / `::warning::` annotations on the GitHub PR diff. |
| `sarif` | GitHub Code Scanning, Azure DevOps, any SARIF v2.1.0 consumer. |
| `dot` | Graphviz module dependency graph. |
| `html` | Self-contained HTML report. Publishable as CI artifact. |
| `ai`, `ai-json` | Compact representations tuned for LLM agents. |

`--json` is shorthand for `--format json`.

## `text` (default)

```
── src/order.rs
  ✓ INTEGRATION process_order (line 12)
  ✓ OPERATION   calculate_discount (line 28)
  ✗ VIOLATION   process_payment (line 48) [MEDIUM]

═══ Summary ═══
  Functions: 24    Quality Score: 82.3%

  IOSP:           85.7%
  Complexity:     90.0%
  DRY:            95.0%
  SRP:           100.0%
  Test Quality:  100.0%
  Coupling:      100.0%
  Architecture:  100.0%

4 quality findings. Run with --verbose for details.
```

`--verbose` adds every function with metrics. `--findings` collapses to one finding per line for piping.

## `json`

Full structured output. Top-level keys:

```jsonc
{
  "version": "1.2.0",
  "summary": { "score": 82.3, "functions": 24, "findings": 4, "warnings": 0,
               "dimensions": { "iosp": 85.7, "complexity": 90.0, /* ... */ } },
  "findings": [
    { "code": "iosp/violation", "severity": "medium",
      "file": "src/order.rs", "line": 48, "function": "process_payment",
      "message": "function mixes orchestration with logic" }
  ],
  "files": [ /* per-file analysis */ ],
  "config": { /* effective config */ }
}
```

Use this for custom dashboards, regression tracking, or piping into shell tooling:

```bash
rustqual --format json | jq '.summary.score'
rustqual --format json | jq '.findings[] | select(.severity == "high")'
```

## `github`

GitHub Actions workflow-command annotations. Inline on the PR diff:

```
::error file=src/order.rs,line=48,title=IOSP::function mixes orchestration with logic
::warning file=src/utils/legacy.rs,line=12,title=DRY-002::dead code
```

Combine with `--diff origin/main` for PR-only analysis:

```yaml
- run: rustqual --diff origin/main --format github
```

## `sarif`

SARIF v2.1.0. Designed for GitHub Code Scanning, but consumed by Azure DevOps, Sonatype, and any SARIF tool.

```yaml
- run: rustqual --format sarif > rustqual.sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: rustqual.sarif
```

Findings show up in the **Security** tab as Code Scanning alerts. Each rule has a stable rule ID (`CX-001`, `DRY-002`, etc.) so dismissals persist across runs.

## `dot`

Module dependency graph in Graphviz format:

```bash
rustqual --format dot | dot -Tpng -o deps.png
rustqual --format dot | dot -Tsvg -o deps.svg
```

Useful for spotting cycles or visualising layer separation. Pair with `coupling-quality.md`.

## `html`

Self-contained HTML report — no external CSS/JS, no network required. Embed in CI as an artifact:

```yaml
- run: rustqual --format html > quality.html
- uses: actions/upload-artifact@v4
  with:
    name: quality-report
    path: quality.html
```

The HTML report includes:

- Per-dimension scores with sparklines.
- Sortable / filterable findings table.
- Per-file drilldown.
- Per-function metrics.

## `ai` / `ai-json`

Compact representations tuned for LLM consumption — fewer tokens than full JSON, focused on what an agent needs to act:

- `ai` — token-efficient text format. Findings only, with file:line, code, and a one-line description.
- `ai-json` — minimal JSON: code, file, line, function, message. No metadata, no per-file tables.

Useful when you're piping rustqual output into a coding agent (Claude Code, Cursor, etc.) and want to keep the prompt small.

```bash
rustqual --format ai | claude code "Fix these findings"
```

## Choosing a format

| Audience | Format |
|---|---|
| Developer at a terminal | `text` |
| GitHub PR reviewer | `github` |
| GitHub Code Scanning | `sarif` |
| Custom CI dashboard | `json` |
| Architecture review meeting | `dot` (rendered to PNG/SVG) |
| Stakeholder report | `html` artifact |
| LLM agent prompt | `ai` / `ai-json` |

Most CI configurations use `github` or `sarif` (or both). Local development: `text`. Tooling integration: `json`.

## Related

- [reference-cli.md](./reference-cli.md) — `--format` and other flags
- [ci-integration.md](./ci-integration.md) — CI examples for each format
- [reference-rules.md](./reference-rules.md) — codes referenced in every format
