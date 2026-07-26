# Reference: suppression annotations

Annotation forms, ordered from most-restricted to least:

| Annotation | Scope | Counts against `max_suppression_ratio` |
|---|---|---|
| `// qual:allow(<dim>, <target>[=N]) reason: "…"` | One finding-kind within a multi-kind dimension (`complexity`, `dry`, `srp`, `coupling`, `test_quality`, `architecture`) | Yes |
| `// qual:allow(iosp)` | The whole `iosp` dimension (its only form — `iosp` has no targets) | Yes |
| `// qual:allow(unsafe)` | `CX-006` only | No |
| `// qual:api` | Excludes from `DRY-002`, `DRY-006`, `TQ-003` | No |
| `// qual:test_helper` | Excludes from `DRY-002` / `DRY-006` (test-only), `TQ-003` | No |
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

- `DRY-002` (dead code) — the function isn't dead, it's exported.
- `DRY-006` (dead type) — same, for a type or constant (since 1.8.0).
- `TQ-003` (untested) — the function may be tested by downstream consumers.

Other dimensions still apply (complexity, IOSP, etc.). Doesn't count against `max_suppression_ratio`.

**It is verified (since 1.7.0, on types since 1.8.0).** The marker excuses a
declaration *production never uses*, so rustqual reports it as
`ORPHAN_SUPPRESSION` once that stops being true:

| Situation | What you get |
|---|---|
| Reachable from outside the crate, nothing in the workspace uses it | Silent — the marker is doing its job |
| Production calls the function / refers to the type | *spent* — remove the marker (an `untested` finding may surface; that is the point) |
| Item can't be named from outside (private `mod`, not `pub`, or in a binary) | *never applied* — remove the marker, and use it from production or delete it |
| The marker reaches no declaration at all (a `pub use`, a module, a trait) | *not attached* — it silences nothing; remove it |

Reachability is derived from the sources alone: the `mod` visibility chain
from a library root plus `pub use` re-exports. Anything rustqual cannot
resolve counts as reachable, so an unusual layout never produces a false
finding.

## `// qual:test_helper` — `src/` helpers used only from `tests/`

```rust
// qual:test_helper
pub fn assert_in_range(actual: f64, expected: f64, tol: f64) {
    assert!((actual - expected).abs() < tol);
}
```

Same exclusions as `qual:api` (`DRY-002`, `DRY-006`, `TQ-003`). Use when a helper — a function, or since 1.8.0 a type — lives in `src/` so it's importable from integration tests in `tests/`, but isn't used from any production code.

Unlike a blanket exclusion, `qual:test_helper` only silences DRY-002 and TQ-003 — complexity / SRP / IOSP all still apply. (rustqual has no function-name ignore list; the blunt `ignore_functions` option was removed in 1.5.0.)

Verified too (since 1.7.0): if **production** calls the helper it is no longer
test-only, and the marker is reported as *spent*. Being unreachable from
outside the crate is normal here and never reported. A helper that *nothing*
calls — not even a test — still surfaces as `DRY-002` dead code, because this
marker deliberately does not silence the `uncalled` variant.

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

The `//!` form attaches to the module, not to a single item. The pin re-fires if instability climbs past it. A coupling marker for a **module-global** target — the `max_fan_in`/`max_fan_out`/`max_instability` metrics or the boolean `sdp` check — has no line-anchored finding, so it is *not* orphan- or too-loose-checked (a deliberate blind spot — see Orphan detection). The **structural** coupling targets (`oi`/`sit`/`deh`/`iet`) are line-anchored and *are* orphan-checked like any other target.

## Suppression ratio (`SUP-001`)

The `max_suppression_ratio` config (default 5%) caps how much of the codebase can be under `qual:allow`. Once you exceed that ratio, `SUP-001` warns (or errors with `--fail-on-warnings`).

The intent is that suppression is for legitimate exceptions, not a backdoor. If you're hitting the cap, the right response is either:

- Refactor the suppressed functions, or
- Loosen a threshold so the underlying findings stop firing legitimately, or
- Raise `max_suppression_ratio` *with intent* (and document why in `rustqual.toml`).

Don't silently raise it to make the warning go away. The whole point of the cap is to keep suppression from drifting upward over months.

## Orphan detection (`ORPHAN-001`)

A `// qual:allow(...)` marker emits `ORPHAN-001` when it is *stale* — doesn't match a finding in its window — or *too-loose* (a metric pin sitting too far above the value it covers; see below). The stale case catches annotations left behind after a refactor — the underlying issue is gone, but the suppression is still there.

The detector reads raw complexity metrics against config thresholds, not the `*_warning` flags that suppressions clear. So if you bump a threshold, the finding stops firing, *and* the orphan check then flags the now-redundant suppression. Coupling markers for a module-global target (the `max_fan_in`/`max_fan_out`/`max_instability` metrics or the boolean `sdp` check) are skipped — those warnings are module-global with no line anchor; the structural coupling targets (`oi`/`sit`/`deh`/`iet`) are line-anchored and orphan-checked like any other.

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
