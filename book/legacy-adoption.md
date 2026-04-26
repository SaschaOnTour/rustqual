# Use case: adopting rustqual on an existing codebase

Running rustqual against a large legacy Rust codebase for the first time will produce a lot of findings. That's expected — rustqual was built around an opinionated set of structural rules (IOSP especially). The trick is not to fix everything at once. This guide shows four adoption patterns ordered from "lightest touch" to "full enforcement".

## The general principle

You don't have to enable everything on day one. Each dimension has an `enabled` flag, and you can ratchet up over time. A typical adoption sequence:

1. Start with **DRY** and **Test Quality** — high-signal, low-controversy findings (duplicates, dead code, untested functions, weak assertions).
2. Add **Complexity** — function length, magic numbers, error-handling patterns. Most findings are quick fixes.
3. Add **SRP** and **Coupling** — module-level structure. Some refactoring required, but each fix is local.
4. Add **Architecture** with layer rules — once you've decided what your layering should look like.
5. Add **IOSP** last. This is the most invasive and benefits most from existing decomposition.

## Pattern A: shallow adoption — defaults off, ratchet on

Disable the dimensions you're not ready for. Start with whatever you do feel ready to enforce:

```toml
# rustqual.toml — minimal initial config
[complexity]
enabled = true
max_function_lines = 80              # initial floor; tighten over time

[duplicates]
enabled = true

[test_quality]
enabled = true

[srp]
enabled = false                       # enable later

[coupling]
enabled = false                       # enable later

[architecture]
enabled = false                       # enable when you've decided on layering
```

If a dimension produces too much noise to act on right now, disable it with `enabled = false` and re-enable later as you ratchet up. For dimensions you can't selectively disable, Pattern B (baseline) absorbs existing findings without enforcing them.

## Pattern B: baseline — accept current state, enforce no regression

The most common adoption pattern for an active codebase. Snapshot the current quality state, then in CI fail only on regression:

```bash
# Generate the baseline once
rustqual --save-baseline baseline.json
git add baseline.json
git commit -m "Add quality baseline"
```

In CI:

```yaml
- run: rustqual --compare baseline.json --fail-on-regression
```

New code must be at least as good as what's there. Existing findings stay, but PRs can't introduce new ones. Regenerate the baseline as part of dedicated refactor PRs:

```bash
rustqual --save-baseline baseline.json   # after refactor lowers the count
```

This works without disabling anything. You get the full set of checks active immediately, but you don't have to refactor everything.

## Pattern C: per-function suppression with rationale

For specific functions you genuinely don't want to refactor (legacy entry points, generated code, etc.), use `// qual:allow` annotations:

```rust
// qual:allow(iosp) — legacy handler from the v1 API; superseded by handler_v2.
// Kept for backward compat until 2027 sunset.
pub fn legacy_dispatch(req: Request) -> Response {
    if req.is_v1() {
        handle_v1(req)
    } else {
        handle_v2(req)
    }
}
```

Pros:
- Explicit, reviewable in PRs.
- The rationale is right next to the code that needs it.
- `max_suppression_ratio` (default 5%) caps how much can be suppressed before rustqual itself complains.

Cons:
- Each function needs an annotation.
- Easy to over-use if you're not careful.

For genuine public-API surface, prefer `// qual:api`:

```rust
// qual:api — public re-export, callers live outside this crate
pub fn encode(data: &[f32]) -> Vec<u8> { /* … */ }
```

This excludes the function from dead-code (DRY-002) and untested-function (TQ-003) detection without counting against the suppression ratio. Other dimensions (complexity, IOSP, etc.) still apply.

For test-only helpers in `src/` that are called from `tests/`:

```rust
// qual:test_helper
pub fn assert_in_range(actual: f64, expected: f64, tol: f64) {
    assert!((actual - expected).abs() < tol);
}
```

Same treatment as `// qual:api` for DRY-002/TQ-003, also no ratio cost.

Full annotation reference: [reference-suppression.md](./reference-suppression.md).

## Pattern D: bulk-suppress a directory

For directories you genuinely don't want to analyse (vendored code, auto-generated files, examples):

```toml
exclude_files = [
    "src/legacy/**",
    "src/generated/**",
    "examples/**",
]
```

These files are skipped entirely — no findings, no ratio cost.

## Recommended onboarding sequence

1. **Day 1.** `cargo install rustqual && rustqual --init`. Read the generated config. Don't change it yet.
2. **Day 1.** Run `rustqual --no-fail`. Look at the dimension scores. Note which dimensions are at 100% already (free wins) and which are far off.
3. **Day 1.** Disable the dimensions that produce noise you can't act on yet. Keep the ones that produce actionable findings.
4. **Week 1.** Add a CI step with `--compare baseline.json --fail-on-regression`. Commit the baseline.
5. **Week 2–4.** Incrementally fix findings or add `// qual:allow` annotations with rationale. Re-baseline after each batch.
6. **Month 2+.** Re-enable disabled dimensions one by one. Tighten thresholds in `rustqual.toml` as you go.
7. **Month 3+.** Switch to `--min-quality-score 90` or similar, and drop the baseline.

This stages the cost over weeks instead of front-loading it on day one.

## Things that often surprise people

**IOSP scores are usually low at first.** A typical Rust codebase has 30-60% IOSP compliance before refactoring. Don't panic — that's the dimension's whole point. The `--suggestions` flag gives pattern-based hints for fixing common cases.

**The Architecture dimension defaults to "strict_error" for unmatched files.** If your `[architecture.layers]` globs don't cover every production file, rustqual will tell you. Either widen the globs, mark the file as a re-export point, or set `unmatched_behavior = "composition_root"` to opt out of strict mode.

**`--init` produces a config tailored to your current metrics.** It's intentional: starting with realistic thresholds means most findings are real ones, not aspirational ones.

## Related

- [ci-integration.md](./ci-integration.md) — for `--compare` and `--fail-on-regression` CI patterns
- [reference-suppression.md](./reference-suppression.md) — full annotation reference
- [reference-configuration.md](./reference-configuration.md) — every config option
