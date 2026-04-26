# Use case: function-level quality

Function quality is where most code rot starts. A function that mixes orchestration with logic, or that grows past 60 lines with deep nesting, is the smell that everything else builds on. rustqual catches these mechanically — through IOSP, complexity metrics, and method-shape checks — so the feedback loop runs at every change instead of every six months.

## IOSP — Integration vs Operation

Every function should be either:

- **Integration** — orchestrates other functions. No own logic (no `if`, `for`, `match`, arithmetic, etc.). Just calls and assignments.
- **Operation** — contains logic. No calls into your own project's functions (stdlib and external crates are fine).

A function that does both is a **Violation** — that's the smell to fix. Splitting it gives you one Integration and one or more Operations, each with a single purpose.

```rust
// Violation: mixes orchestration and logic
pub fn process_order(order: Order) -> Result<Receipt> {
    let total = order.items.iter().map(|i| i.price).sum::<f64>();   // logic
    let discount = if order.is_premium { 0.1 } else { 0.0 };         // logic
    let final_total = total * (1.0 - discount);                       // logic
    let receipt = generate_receipt(&order, final_total)?;             // call
    save_receipt(&receipt)?;                                          // call
    Ok(receipt)
}

// After refactor: one Integration, two Operations
pub fn process_order(order: Order) -> Result<Receipt> {
    let total = calculate_total(&order);
    let receipt = generate_receipt(&order, total)?;
    save_receipt(&receipt)?;
    Ok(receipt)
}

fn calculate_total(order: &Order) -> f64 {
    let raw = order.items.iter().map(|i| i.price).sum::<f64>();
    let discount = if order.is_premium { 0.1 } else { 0.0 };
    raw * (1.0 - discount)
}
```

### Leniency rules

Out of the box, IOSP is forgiving where it matters:

- **Closures and async blocks** don't count as logic. `.map(|x| x.foo())` is fine inside an Integration.
- **For-loops over delegation** — a `for x in xs { handler(x) }` is treated as orchestration, not logic.
- **Match-as-dispatch** — a `match` whose every arm is a single delegation call is orchestration. Add a guard or any logic to one arm and it becomes a Violation.
- **Trivial self-getters** (`fn name(&self) -> &str { &self.name }`) are excluded from own-call counting.
- **`#[test]` functions** are exempt from IOSP — assertions are logic by nature.

Tighten with `--strict-closures` / `--strict-iterators` if you want closures and iterator chains to count as logic.

## Complexity

IOSP shapes the *structure* of functions; complexity caps the *size* of each piece. The defaults are deliberate:

| Rule | Default | What it catches |
|---|---|---|
| `CX-001` | cognitive ≤ 15 | Hard-to-read control flow (deep nesting + boolean combinators). Cognitive penalises nesting more than cyclomatic does. |
| `CX-002` | cyclomatic ≤ 10 | Decision-point density. |
| `CX-003` | magic number ≤ 0 | Bare numeric literals outside `const`/`static`. Forces named constants. |
| `CX-004` | length ≤ 60 lines | Function size. |
| `CX-005` | nesting ≤ 4 levels | Pyramid-of-doom guard. |
| `CX-006` | `detect_unsafe = true` | `unsafe { … }` blocks. Use `// qual:allow(unsafe)` for genuine FFI. |
| `A20` | `detect_error_handling = true` | `.unwrap()`, `.expect()`, `panic!`, `todo!` in production code. |

Configure thresholds in `rustqual.toml`:

```toml
[complexity]
enabled = true
max_cognitive = 15
max_cyclomatic = 10
max_function_lines = 60
max_nesting_depth = 4
detect_unsafe = true
detect_error_handling = true
allow_expect = false   # set true to permit .expect() but still flag .unwrap()
```

`--init` calibrates these to your current metrics so the initial run produces actionable findings, not aspirational ones.

### Test-aware

`A20` and `CX-004` skip `#[test]` functions and files under workspace-root `tests/**`. Asserting on `.unwrap()` in a test is fine; it's panicking in production that matters.

## Method-shape checks

Beyond IOSP and complexity, rustqual flags structural smells at the method level. These are part of the **structural binary checks** under SRP/Coupling:

| Rule | What it means |
|---|---|
| `SLM` | **Selfless method** — takes `self` but never references it. Should be a free function or associated function. |
| `NMS` | **Needless `&mut self`** — declares mutable receiver but never mutates. Tighten the signature to `&self`. |
| `BTC` | **Broken trait contract** — every method in an `impl Trait` is a stub (`unimplemented!`, `todo!`, `Default::default()`). The trait is unimplemented in spirit. |

Configure under `[structural]`:

```toml
[structural]
enabled = true
# check_btc = true
# check_slm = true
# check_nms = true
```

## Parameter sprawl

`SRP-003` flags functions with too many parameters (default: > 5). The fix is usually a context struct:

```rust
// Flagged
fn render(width: u32, height: u32, dpi: u32, theme: Theme,
          locale: Locale, watermark: Option<&str>) { /* … */ }

// Better
fn render(opts: &RenderOptions) { /* … */ }
```

## What you'll see

```
✗ VIOLATION   process_order (line 48) [MEDIUM]
                IOSP — function mixes orchestration with logic
                CX-001  cognitive=18 > 15
                CX-004  length=72 > 60

⚠ SLM         dispatch (line 124) — takes &self but never uses it
```

`--findings` gives one finding per line for piping; `--verbose` shows every function with its full metrics.

## Suppression for legitimate exceptions

For functions you genuinely cannot refactor right now (legacy entry points, generated code, FFI shims):

```rust
// qual:allow(iosp) — match-dispatcher; arms intentionally inlined for codegen
fn dispatch(cmd: Command) -> Result<()> { /* … */ }

// qual:allow(complexity) — large lookup table; splitting hurts readability
fn rule_table() -> &'static [Rule] { /* … */ }

// qual:allow(unsafe) — FFI boundary, audited 2026-Q1
unsafe fn raw_call() { /* … */ }
```

Suppressions count against `max_suppression_ratio` (default 5%) so they can't silently take over the codebase. `unsafe`-specific suppression has a separate path that doesn't count against that ratio.

Full annotation reference: [reference-suppression.md](./reference-suppression.md).

## Why IOSP for AI-assisted code

AI agents tend to inline everything — they have no internal pressure to decompose. IOSP makes that pressure mechanical: the agent writes a god-function, rustqual marks it, the agent reads the finding and splits. That converges the loop on small, single-purpose functions instead of "works but unmaintainable" code that passes tests and clippy but rots over months.

Background on the principle itself: [flow-design.info](https://flow-design.info/) — Ralf Westphal's original write-up on Integration Operation Segregation.

## Related

- [module-quality.md](./module-quality.md) — when too many functions cluster into one module
- [test-quality.md](./test-quality.md) — assertion density, untested functions
- [code-reuse.md](./code-reuse.md) — duplicates and dead code
- [reference-rules.md](./reference-rules.md) — every rule code with details
- [reference-configuration.md](./reference-configuration.md) — every config option
