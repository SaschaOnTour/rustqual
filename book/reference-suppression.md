# Reference: suppression annotations

Annotation forms, ordered from most-restricted to least:

| Annotation | Scope | Counts against `max_suppression_ratio` |
|---|---|---|
| `// qual:allow(<dim>, <target>[=N]) reason: "…"` | One finding-kind within a multi-kind dimension (`complexity`, `dry`, `srp`, `coupling`, `test_quality`, `architecture`) | Yes |
| `// qual:allow(iosp)` | The whole `iosp` dimension (its only form — `iosp` has no targets) | Yes |
| `// qual:allow(unsafe)` | `CX-006` only | No |
| `// qual:api` | Excludes from `DRY-002`, `TQ-003` | No |
| `// qual:test_helper` | Excludes from `DRY-002` (testonly), `TQ-003` | No |
| `// qual:inverse(<fn>)` | Suppresses near-duplicate `DRY-001` for inverse pairs | No |
| `// qual:recursive` | Removes self-calls from own-calls before leaf reclassification | No |

Each annotation lives in a `//`-comment block immediately above the item it applies to. The block extends upward until a blank line or a non-`//` line breaks it. `#[derive(...)]` and other attributes between the comment block and the item are fine — they don't break the block.

**Removed in 1.2.3:** the bare `// qual:allow` (no parens) and `// qual:allow()` forms no longer suppress anything — they're silently ignored.

**Breaking in 1.5.0 ("the flip"):** a bare `// qual:allow(<dim>)` is **rejected** for every dimension that has targets (`complexity`, `dry`, `srp`, `coupling`, `test_quality`, `architecture`) — it would silence *every* finding of that dimension, too blunt. You must name a target; the error lists the valid ones. Only `iosp` (no targets) keeps its bare form. Multi-dimension blanket markers (`allow(a, b)`) are gone — use one marker per dimension. Typos like `// qual:allow(srp_params)` (no recognised dimension) surface as `ORPHAN_SUPPRESSION` so they don't quietly hide nothing.

## `// qual:allow(<dim>, <target>[=N])` — suppress one finding-kind

Each multi-kind dimension exposes a **vocabulary of targets** (`rustqual --explain allow` lists them all). A *boolean* target takes no value; a *metric* target requires a pinned ceiling `=N` and re-fires once the value climbs above it. Every targeted suppression needs a `reason:`.

```rust
// qual:allow(complexity, max_cyclomatic=12) reason: "Kosaraju three-pass is inherently branchy"
fn detect_cycles(g: &Graph) -> Vec<Cycle> { /* … */ }

// qual:allow(srp, file_length=400) reason: "long but cohesive single normalizer"
mod normalizer { /* … */ }

// qual:allow(architecture, forbidden) reason: "audited port→registry edge for round-trip ordering"
use crate::adapters::registry::lookup;

// iosp is the one single-kind dimension — bare form only:
// qual:allow(iosp) — match dispatcher; arms intentionally inlined
fn dispatch(cmd: Command) -> Result<()> {
    match cmd { Command::Sync => sync_handler(), Command::Diff => diff_handler() }
}
```

The `reason:` is mandatory for every targeted marker — reviewers and future-you need to know *why*, and a metric pin parked too far above the real value is itself reported (see Orphan detection). Legacy `// iosp:allow` is an alias for `// qual:allow(iosp)`.

## `// qual:allow(unsafe)` — for `CX-006` specifically

```rust
// qual:allow(unsafe) — FFI boundary, audited 2026-Q1
unsafe fn raw_call() { /* … */ }
```

Separate path that does *not* count against `max_suppression_ratio`. Intended for FFI shims and low-level optimisations where `unsafe` is intrinsic.

## `// qual:api` — public API entry points

```rust
// qual:api — public re-export, callers live outside this crate
pub fn parse_config(input: &str) -> Result<Config> { /* … */ }
```

Excludes from:

- `DRY-002` (dead code) — function isn't dead, it's exported.
- `TQ-003` (untested) — function may be tested by downstream consumers.

Other dimensions still apply (complexity, IOSP, etc.). Doesn't count against `max_suppression_ratio`.

## `// qual:test_helper` — `src/` helpers used only from `tests/`

```rust
// qual:test_helper
pub fn assert_in_range(actual: f64, expected: f64, tol: f64) {
    assert!((actual - expected).abs() < tol);
}
```

Same exclusions as `qual:api` (`DRY-002`, `TQ-003`). Use when a helper lives in `src/` so it's importable from integration tests in `tests/`, but isn't called from any production code.

Unlike a blanket exclusion, `qual:test_helper` only silences DRY-002 and TQ-003 — complexity / SRP / IOSP all still apply. (rustqual has no function-name ignore list; the blunt `ignore_functions` option was removed in 1.5.0.)

## `// qual:inverse(<fn>)` — inverse method pairs

```rust
// qual:inverse(decode)
pub fn encode(input: &Value) -> Vec<u8> { /* … */ }

// qual:inverse(encode)
pub fn decode(input: &[u8]) -> Result<Value> { /* … */ }
```

Suppresses the near-duplicate `DRY-001` finding between encode/decode pairs whose structural similarity is intentional. The annotation must be reciprocal — both functions name each other. Doesn't count against `max_suppression_ratio`.

## `// qual:recursive` — recursive helpers

```rust
// qual:recursive
fn collect_descendants(node: &Node, out: &mut Vec<Id>) {
    out.push(node.id);
    for child in &node.children {
        collect_descendants(child, out);   // self-call: ignored for IOSP reclassification
    }
}
```

Removes self-calls from `own_calls` before the leaf-reclassification pass. Useful for tree/graph helpers that don't otherwise violate IOSP. Doesn't count against `max_suppression_ratio`.

## Module-level suppression

*Coupling* findings are module-global, not function-local, so a coupling marker applies to the whole module. Use the inner-doc form, and — since coupling has targets — name the metric you mean (a bare `allow(coupling)` is rejected by the flip):

```rust
//! qual:allow(coupling, max_instability=0.95) reason: "orchestration layer, intentionally depends on every adapter."

use crate::adapters::a;
use crate::adapters::b;
// …
```

The `//!` form attaches to the module, not to a single item. The pin re-fires if instability climbs past it. Note that targeted coupling markers are module-global and have no line-anchored finding, so they are *not* orphan- or too-loose-checked (a deliberate blind spot — see Orphan detection).

## Suppression ratio (`SUP-001`)

The `max_suppression_ratio` config (default 5%) caps how much of the codebase can be under `qual:allow`. Once you exceed that ratio, `SUP-001` warns (or errors with `--fail-on-warnings`).

The intent is that suppression is for legitimate exceptions, not a backdoor. If you're hitting the cap, the right response is either:

- Refactor the suppressed functions, or
- Loosen a threshold so the underlying findings stop firing legitimately, or
- Raise `max_suppression_ratio` *with intent* (and document why in `rustqual.toml`).

Don't silently raise it to make the warning go away. The whole point of the cap is to keep suppression from drifting upward over months.

## Orphan detection (`ORPHAN-001`)

A `// qual:allow(...)` marker that *doesn't match a finding in its window* emits `ORPHAN-001`. This catches stale annotations after a refactor — the underlying issue is gone, but the suppression is still there.

The detector reads raw complexity metrics against config thresholds, not the `*_warning` flags that suppressions clear. So if you bump a threshold, the finding stops firing, *and* the orphan check then flags the now-redundant suppression. Coupling-only markers are skipped because coupling warnings are module-global.

**Target-aware.** A *targeted* marker is checked against a finding of its **own** kind: a `// qual:allow(srp, file_length=400)` parked next to a god-struct finding (but no module-length finding) is still an orphan — the unrelated finding doesn't satisfy it. A blanket marker (where the dimension allows one, i.e. `iosp`) still matches any finding of its dimension.

**Too-loose pins.** A metric pin that *does* cover its finding but sits too far above the actual value is reported as a too-loose orphan: *"pin N sits >10% above the actual value V — tighten to ~V or remove."* The threshold is `[suppression].pin_headroom` (default `0.10`). This stops a pin parked far above the real metric from silently absorbing regressions up to its ceiling. A pin *below* its value (one that re-fires) is left alone — it is legitimately limiting, not stale.

`ORPHAN-001` is visible in every output format and counts toward `total_findings()`, `--fail-on-warnings`, and default-fail.

## Composition-root and reexport-point modules

Files marked as reexport points in `[architecture.reexport_points]` (typically `lib.rs`, `main.rs`, `bin/**`, `cli/**`, `tests/**`) bypass the layer rule entirely — no `qual:allow` needed. Use this for the composition root that wires layers together.

## Summary: when to use which

| You want… | Use |
|---|---|
| To skip one finding-kind in production code | `// qual:allow(<dim>, <target>)` with `reason:` (or bare `// qual:allow(iosp)`) |
| To mark a function as exposed externally | `// qual:api` |
| To mark a `src/` helper used only from `tests/` | `// qual:test_helper` |
| To accept structurally-similar inverse pairs | `// qual:inverse(<peer>)` |
| To allow a recursive helper through IOSP | `// qual:recursive` |
| To accept FFI / `unsafe` blocks | `// qual:allow(unsafe)` |
| To exempt a module's coupling metric | `//! qual:allow(coupling, max_instability=N)` with `reason:` |

## Related

- [reference-rules.md](./reference-rules.md) — every rule code each annotation can suppress
- [reference-configuration.md](./reference-configuration.md) — `max_suppression_ratio`, `exclude_files`, layer rules
- [legacy-adoption.md](./legacy-adoption.md) — adoption patterns using suppressions vs baselines
