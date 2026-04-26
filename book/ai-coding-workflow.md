# Use case: AI-assisted Rust development

This is what rustqual was originally built for. AI coding agents — Claude Code, Cursor, GitHub Copilot, Codex — are productive but consistently produce a recognisable set of structural smells. rustqual catches them mechanically so the agent can self-correct, without you having to spot every issue in code review.

## What AI agents tend to get wrong

- **God-functions** — functions that mix orchestration with logic ("call helper, then if/else, then call another helper, then …"). Hard to test, hard to read, hard to refactor.
- **Long functions with deep nesting** — agents err on the side of inlining everything they need. Cognitive complexity climbs fast.
- **Copy-paste with minor variation** — when asked to "do the same for X", agents often copy the implementation rather than extracting a shared abstraction.
- **Tests without assertions** — agents generate test bodies that *exercise* code without *checking* it. Coverage looks good, real coverage is zero.
- **Architectural drift** — adding code "wherever it fits" instead of respecting the project's layering. The domain layer slowly imports adapters, infrastructure leaks into application, etc.
- **Asymmetric adapters** — when a project has multiple frontends (CLI, REST, MCP), agents tend to wire new functionality into the one they're touching and forget the others.

rustqual catches all of these. The trick is wiring it into the agent's feedback loop so it self-corrects.

## Pattern 1: agent instruction file

Drop this into `CLAUDE.md`, `.cursorrules`, `.github/copilot-instructions.md`, or whichever instruction file your tool reads:

```markdown
## Code Quality Rules

- Run `rustqual` after making changes. All findings must be resolved before marking a task complete.
- Follow IOSP: every function is either an Integration (calls other functions, no own logic) or an Operation (contains logic, no calls to project functions). Never mix both.
- Keep functions under 60 lines and cognitive complexity under 15.
- Don't duplicate logic — extract shared patterns into reusable Operations.
- Don't introduce functions with more than 5 parameters.
- Every test function must contain at least one assertion (`assert!`, `assert_eq!`, etc.).
- For public-API functions that are intentionally untested in this crate, mark with `// qual:api` instead of writing a stub test.
```

The agent gets actionable feedback: rustqual tells it which function violated which principle, so it can self-correct without you having to point each issue out.

## Pattern 2: pre-commit hook

Catch violations before they enter version control — useful when the agent runs locally:

```bash
#!/bin/bash
# .git/hooks/pre-commit
if ! rustqual 2>/dev/null; then
    echo "rustqual: quality findings detected. Refactor before committing."
    exit 1
fi
```

Make it executable: `chmod +x .git/hooks/pre-commit`.

This gives the agent immediate feedback before anything reaches the remote. If you're using Claude Code with hooks (`PostToolUse`), you can wire `rustqual` into the same loop: every Edit triggers a re-check.

## Pattern 3: CI quality gate

```yaml
# .github/workflows/quality.yml
name: Quality Check
on: [pull_request]

jobs:
  quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install rustqual cargo-llvm-cov
      - run: cargo llvm-cov --lcov --output-path lcov.info
      - run: rustqual --diff HEAD~1 --coverage lcov.info --format github
```

`--format github` produces inline annotations on the PR diff — exactly where the issue is, what rule fired, why it matters. `--diff HEAD~1` restricts analysis to the changed files so PRs stay fast even on large codebases.

## Pattern 4: baseline tracking for AI-velocity codebases

If you have a codebase already at the limit of what you can refactor right now, but you want to make sure new AI-generated code doesn't make it worse:

```bash
# Snapshot the current state
rustqual --save-baseline baseline.json

# In CI: fail only on regression
rustqual --compare baseline.json --fail-on-regression
```

This lets you ratchet quality up over time without blocking PRs that don't make things worse. Combined with `--min-quality-score 90`, you get a hard floor plus a no-regression rule — exactly what you want when an agent is generating dozens of PRs a week.

## Why IOSP specifically

The Integration/Operation distinction is what separates rustqual from a generic linter for AI-coding contexts. AI agents naturally produce mixed-concern functions — they don't have an internal pressure to decompose. IOSP makes that pressure mechanical: the agent writes a god-function, rustqual marks it as a violation, the agent reads the finding, splits the function. Repeat until the loop converges on small, single-purpose functions.

Without that constraint, agents settle into "works but unmaintainable" code that passes tests, passes clippy, and rots over six months. With it, the agent is structurally pushed toward decomposition every time.

## Suppression for legitimate exceptions

Not every violation is a bug. Use `// qual:allow` annotations sparingly:

```rust
// qual:allow(iosp) — match dispatcher; splitting would just rename the match
fn dispatch(cmd: Command) -> Result<()> {
    match cmd {
        Command::Sync => sync_handler(),
        Command::Diff => diff_handler(),
        // …
    }
}
```

The `max_suppression_ratio` config (default 5%) caps how much code can be suppressed. If the agent suppresses too much, that itself becomes a finding.

Full annotation reference: [reference-suppression.md](./reference-suppression.md).

## Related

- [function-quality.md](./function-quality.md) — what IOSP/complexity actually check
- [test-quality.md](./test-quality.md) — assertion density, coverage, untested functions
- [legacy-adoption.md](./legacy-adoption.md) — applying this to a codebase that's already grown messy
- [ci-integration.md](./ci-integration.md) — full CI patterns
