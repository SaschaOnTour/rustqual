# Use case: test quality

Coverage is a number; *test quality* is whether the tests actually catch regressions. A 95%-covered codebase with assertion-free tests is worse than a 60%-covered one with sharp ones — the former gives false confidence, the latter is honest about what's checked.

rustqual's Test Quality dimension measures both. It runs static checks on every test function (assertion presence, SUT calls, untested production functions) and — when LCOV coverage data is supplied — adds branch-level uncovered-logic detection.

## What goes wrong

- **Tests without assertions.** `#[test] fn it_works() { run_thing(); }` exercises code without checking anything. Coverage looks great, real coverage is zero.
- **Tests that don't call the SUT.** A test in `src/auth/tests/login.rs` that only constructs values and asserts on them — but never calls `login()` — isn't testing anything in `auth`.
- **Untested production functions.** A `pub fn` with no test anywhere — neither unit, nor integration — slipped through the review.
- **Uncovered logic branches.** A function is "tested" but the `else` arm of its main `if` was never executed.

## What rustqual catches

| Rule | Meaning |
|---|---|
| `TQ-001` | Test function has no assertions |
| `TQ-002` | Test function does not call any production function |
| `TQ-003` | Production function is untested (no test anywhere calls it) |
| `TQ-004` | Production function has no coverage (LCOV-based, requires `--coverage`) |
| `TQ-005` | Untested logic branches — covered function with uncovered lines |

`TQ-001`, `TQ-002`, `TQ-003` are static and run on every analysis. `TQ-004` and `TQ-005` need LCOV data.

## Static checks

### TQ-001 — assertion-free tests

A test is anything with `#[test]`, `#[tokio::test]`, etc. rustqual scans the function body for:

- `assert!`, `assert_eq!`, `assert_ne!`, `debug_assert*!` (and configurable extras via `extra_assertion_macros`)
- Custom prefixes — anything starting with `assert*` or `debug_assert*` matches

If none are present, `TQ-001` fires. The fix is usually to add an assertion; if the test exists for compile-time/typecheck reasons only, mark it:

```rust
// qual:allow(test_quality) — compile-time check only, no runtime assertion needed
#[test] fn signatures_compile() {
    let _: fn(&str) -> Result<Ast> = parse;
}
```

### TQ-002 — tests without SUT

A test that constructs values and asserts on them but never calls a production function. Usually a hint that:

- the test is testing the test fixtures, not the system under test, or
- the SUT call moved out during a refactor and the test stayed.

The check looks at all calls in the test body and verifies at least one resolves to a function inside `src/` (excluding test helpers).

### TQ-003 — untested production functions

Reverse-walk: for every `pub fn` in `src/`, is there a test (anywhere) that calls it?

Three escape hatches:

```rust
// qual:api — public-API entry, callers live outside this crate
pub fn parse_config(input: &str) -> Result<Config> { /* … */ }

// qual:test_helper — used only from tests/
pub fn build_test_session() -> Session { /* … */ }

// qual:allow(test_quality) — initialised at startup, untestable in isolation
pub fn install_signal_handlers() { /* … */ }
```

`qual:api` and `qual:test_helper` don't count against `max_suppression_ratio`. They mirror the same annotations under DRY-002 (dead code), since a function that's untested *and* unused is usually one finding category, not two.

## Coverage-based checks

For TQ-004 and TQ-005, supply LCOV coverage data:

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --lcov --output-path coverage.lcov
rustqual --coverage coverage.lcov
```

### TQ-004 — uncovered production functions

A `pub fn` whose lines never executed during tests. Differs from `TQ-003` in that it considers *runtime* coverage, not just static call graph: a function might be statically referenced from a test but the test path never executes its body.

### TQ-005 — uncovered logic branches

The most surgical of the test-quality checks. For functions that *are* covered, find the **uncovered lines** and report them as "untested logic". This is where coverage gaps actually hurt: a 95%-covered function whose 5% is the error-handling branch is one production incident away from a regression.

```
⚠ TQ-005  src/auth/login.rs::authenticate (line 48)
            covered: 22/26 lines (84.6%)
            uncovered branches at: 51, 52, 67, 88
```

## Configure

```toml
[test_quality]
enabled = true
# extra_assertion_macros = ["verify", "check", "expect_that"]
```

For projects with custom assertion DSLs (`mockall`, `proptest`, in-house frameworks), add the macro names to `extra_assertion_macros` so TQ-001 doesn't flag tests using them.

## What you'll see

```
✗ TQ-001  tests/auth_test.rs::test_login (line 12) — no assertions
✗ TQ-002  tests/parser_test.rs::test_ast (line 38) — does not call any production fn
✗ TQ-003  src/api/handlers.rs::cmd_admin_purge (line 89) — untested
⚠ TQ-004  src/utils/format.rs::pad_left (line 22) — no coverage (LCOV)
⚠ TQ-005  src/auth/login.rs::authenticate (line 48) — 4 uncovered branches
```

## Strategy

The TQ rules form a ladder, lightest to strictest:

1. **TQ-001 / TQ-002** — fix first. Cheap, mechanical, eliminates fake tests.
2. **TQ-003** — second. Either write a test, mark `qual:api`, or delete the unused function.
3. **TQ-004** — once you have coverage in CI. Catches functions that compile but never run.
4. **TQ-005** — top of the ladder. Forces real branch-level testing.

Most teams sit at level 2-3 and turn 4-5 on as the coverage culture matures. The dimension's quality score reflects all five, weighted; you don't have to enable every check on day one.

## CI integration

```yaml
- run: cargo install rustqual cargo-llvm-cov
- run: cargo llvm-cov --lcov --output-path lcov.info
- run: rustqual --coverage lcov.info --format github
```

Inline PR annotations show *exactly which test* lacks an assertion or *which production function* is uncovered. The agent or developer can fix the specific gap rather than guess from a coverage percentage.

## Why this matters for AI-generated code

AI agents are notorious for generating tests that exercise code without checking it — `let result = foo(); println!("{result:?}");` looks like a test, passes the type-checker, and bumps the coverage number. `TQ-001` catches that mechanically every PR.

Pair it with the agent instruction file pattern from [ai-coding-workflow.md](./ai-coding-workflow.md):

> Every test function must contain at least one assertion (`assert!`, `assert_eq!`, etc.).
> For public-API functions that are intentionally untested in this crate, mark with `// qual:api` instead of writing a stub test.

The agent self-corrects; reviewer time goes to the actual logic, not to spotting fake tests.

## Related

- [function-quality.md](./function-quality.md) — IOSP and complexity for the production code being tested
- [code-reuse.md](./code-reuse.md) — `DRY-002` (dead code) and `TQ-003` (untested) share the call graph
- [ai-coding-workflow.md](./ai-coding-workflow.md) — agent instruction template that includes assertion rules
- [reference-rules.md](./reference-rules.md) — every rule code with details
- [reference-suppression.md](./reference-suppression.md) — `qual:api`, `qual:test_helper`, `qual:allow(test_quality)`
