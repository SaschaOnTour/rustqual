# Reference: output formats

`--format <FMT>` switches the output format. All formats render the same underlying analysis — they differ only in serialisation.

| Format | Use case |
|---|---|
| `text` (default) | Local exploration, terminal use. Coloured summary. |
| `json` | Machine-readable, full detail. Pipe to `jq`, custom dashboards. |
| `github` | `::error::` / `::warning::` annotations on the GitHub PR diff. |
| `sarif` | GitHub Code Scanning, Azure DevOps, any SARIF v2.1.0 consumer. |
| `dot` | Graphviz per-function IOSP call graph (coloured by classification). |
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

Full structured output. Each finding type has its own dimension-specific
section so consumers can pivot per dimension instead of filtering one
flat array. Top-level keys:

```jsonc
{
  "summary": {
    "total": 24, "integrations": 12, "operations": 8, "violations": 2, "trivial": 2,
    "iosp_score": 0.91, "quality_score": 0.823,
    "dimension_scores": [0.91, 0.95, 0.88, 1.0, 1.0, 1.0, 1.0],
    "complexity_warnings": 1, "duplicate_groups": 1, "dead_code_warnings": 1,
    "tq_no_assertion_warnings": 1, "architecture_warnings": 0,
    "orphan_suppressions": 0, "all_suppressions": 3,
    "suppression_ratio_exceeded": false
    /* ... per-dimension counters ... */
  },
  "functions": [
    { "name": "process_payment", "file": "src/order.rs", "line": 48,
      "parent_type": null, "classification": "violation",
      "severity": "medium", "suppressed": false,
      "logic": [{ "kind": "if", "line": "50" }],
      "calls": [{ "name": "log", "line": "53" }],
      "parameter_count": 2, "is_trait_impl": false }
  ],
  "coupling": { "modules": [/* … */], "cycles": [], "sdp_violations": [] },
  "duplicates": [/* … */],
  "dead_code":  [/* … */],
  "fragments":  [/* … */],
  "boilerplate": [/* … */],
  "wildcard_warnings": [/* … */],
  "tq_warnings": [/* … */],
  "structural_warnings": [/* … */],
  "repeated_matches": [/* … */],
  "srp": { "struct_warnings": [/* … */], "module_warnings": [/* … */],
           "param_warnings": [/* … */] },
  "architecture_findings": [
    { "rule_id": "architecture/call_parity/no_delegation",
      "severity": "medium", "file": "src/cli/handlers.rs", "line": 17,
      "message": "cli pub fn delegates to no application function",
      "suppressed": false }
  ],
  "orphan_suppressions": [/* … */]
}
```

Use this for custom dashboards, regression tracking, or piping into shell tooling:

```bash
rustqual --format json | jq '.summary.quality_score'
rustqual --format json | jq '.architecture_findings[] | select(.severity == "high")'
```

## `github`

GitHub Actions workflow-command annotations. Inline on the PR diff:

```
::error file=src/order.rs,line=48::IOSP violation: logic=[if (line 50)], calls=[helper (line 53)]
::warning file=src/utils/legacy.rs,line=12::Dead code detected: legacy::unused
::warning file=src/payment.rs,line=88::Stale qual:allow(complexity) marker — no finding in window.
```

The annotation format is `::{level} file=<path>,line=<n>::{message}` —
GitHub does not show a structured rule-code title for these formats,
so the rule context is folded into the message text. The trailing
summary annotation (`::error::Quality analysis: N finding(s)…` or
`::notice::Quality score: …`) reflects the **default-fail** outcome
of the run. For per-rule filtering in CI, use `--format sarif` and
upload to Code Scanning instead.

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

Per-function IOSP call graph in Graphviz format. Nodes are functions
(coloured by classification: integrations blue, operations green,
violations red, trivial grey); edges follow `own_calls` from each
function's body.

```bash
rustqual --format dot | dot -Tpng -o callgraph.png
rustqual --format dot | dot -Tsvg -o callgraph.svg
```

Useful for spotting integrations that orchestrate too much, violations
clustered around a hub, or trivial leaves that could be inlined. For
module-level dependency cycles use the `coupling` section in the text
or HTML report instead.

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

- Per-dimension score cards (one per dimension) at the top.
- Collapsible `<details>` sections per dimension with the full
  finding tables (IOSP, Complexity, DRY, SRP, Coupling, Test Quality,
  Architecture).
- Per-module coupling table (afferent / efferent / instability).
- Orphan-suppression table when stale `qual:allow` markers exist.

The artifact is fully self-contained — no external CSS, no scripts,
no sortable/filterable interactions. Open it in a browser or embed
it as a CI artifact and read top-to-bottom.

## `ai` / `ai-json`

Compact representations tuned for LLM consumption — fewer tokens than
full JSON, focused on what an agent needs to act:

- `ai` — TOON-encoded version of the same envelope as `ai-json`.
  Token-efficient indented form for prompt embedding.
- `ai-json` — compact (single-line) JSON envelope. Same content as
  the TOON form, just JSON-encoded; both prioritise token efficiency
  for LLM prompts. Example expanded for readability:

  ```jsonc
  {
    "version": "1.2.2",
    "findings": 2,
    "findings_by_file": {
      "src/order.rs": [
        { "category": "violation", "line": 48,
          "fn": "order::process_payment",
          "detail": "logic + calls (logic lines 50, call lines 53)" }
      ],
      "<workspace>": [
        { "category": "cycle", "line": 0, "fn": "",
          "detail": "a -> b -> a" }
      ]
    }
  }
  ```

  The grouping key is the file path (`<workspace>` for findings
  without a file location). Each entry has `category` (a stable
  per-dim slug like `violation`, `cognitive_complexity`, `cycle`,
  `architecture`, `orphan_suppression`), `line`, `fn` (qualified
  function name, empty for module-level findings), and `detail`
  (one-line description of the finding). Same content as `ai`,
  different encoding.

Useful when you're piping rustqual output into a coding agent
(Claude Code, Cursor, etc.) and want to keep the prompt small.

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
