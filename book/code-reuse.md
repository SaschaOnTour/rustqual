# Use case: code reuse (DRY, dead code, boilerplate)

DRY findings are usually the highest-signal category in the first run. They're concrete: this function looks like that function, this code is never called, this `From` impl can be `derive`d. Easy to act on, easy to verify, big quality wins for not much work.

rustqual covers four families:

- **Duplicate functions / fragments / match patterns** — same code in multiple places.
- **Dead code** — defined but never called.
- **Wildcard imports** — `use foo::*;` hiding what's actually pulled in.
- **Boilerplate** — patterns the compiler/macros could write for you.

## What goes wrong

- Two functions that are 90% identical with one parameter-change. Someone copied instead of extracting.
- A helper that nobody calls anymore (refactor leftover, or never wired up).
- `use foo::*;` where you only need `foo::Bar` — you've imported 30 names you don't use, and added 30 hidden coupling points.
- Hand-written `impl Display for FooError` that just maps each variant to a string — `#[derive(thiserror::Error)]` does it automatically.

## What rustqual catches

| Rule | Meaning |
|---|---|
| `DRY-001` | Two functions are duplicates (95%+ token similarity, one is suggested for removal) |
| `DRY-002` | Dead code — function defined but never called |
| `DRY-006` | Dead type or constant — declared but nothing refers to it |
| `DRY-003` | Duplicate code fragment (≥6 lines repeated across functions) |
| `DRY-004` | Wildcard import (`use module::*;`) |
| `DRY-005` | Repeated match pattern across functions (≥3 arms identical, ≥3 instances) |
| `BP-001` | Trivial `From` impl (derivable) |
| `BP-002` | Trivial `Display` impl — semantic: any branch-free, write-only `fmt` body; house idioms declarable via `[boilerplate].accepted_display_idioms` |
| `BP-003` | Trivial getter/setter (consider field visibility) |
| `BP-004` | Builder pattern — consider `derive_builder` or similar |
| `BP-005` | Manual `Default` impl (derivable) |
| `BP-006` | Repetitive match mapping |
| `BP-007` | Error-enum boilerplate (consider `thiserror`) |
| `BP-008` | Clone-heavy conversion |
| `BP-009` | Struct-update boilerplate |
| `BP-010` | Format-string repetition |

## Duplicates

`DRY-001` uses token-based similarity with a 95% threshold by default. It's deliberately strict — finding fewer, higher-confidence duplicates is more useful than spamming "these two functions both call `.unwrap()`". When it fires, it tells you:

- The two functions involved.
- Which one to keep (typically the older / more public).
- The token-level diff between them.

For inverse-method pairs (encode/decode, serialize/deserialize) where structural duplication is intentional:

```rust
// qual:inverse(decode)
pub fn encode(input: &Value) -> String { /* … */ }

// qual:inverse(encode)
pub fn decode(input: &str) -> Value { /* … */ }
```

This suppresses the DRY-001 finding without counting against the suppression ratio.

## Dead code

`DRY-002` builds a workspace-wide call graph. A function that nobody calls — and isn't a `pub` API entry point — is dead code.

Two important escape hatches:

```rust
// qual:api — public re-export, callers live outside this crate
pub fn parse_config(input: &str) -> Result<Config> { /* … */ }

// qual:test_helper — only used from tests/, not from src/
pub fn assert_in_range(actual: f64, expected: f64, tol: f64) { /* … */ }
```

`qual:api` and `qual:test_helper` exclude the declaration from `DRY-002` / `DRY-006` *and* from `TQ-003` (untested), without counting against `max_suppression_ratio`. Use them on things exported to consumers outside the crate or used only by integration tests. Both markers are verified — see [reference-suppression.md](./reference-suppression.md).

By default, the dead-code analysis treats a package's `tests/**` files as call-sites — both the analysis-root crate's `tests/**` and each member's `crates/*/tests/**` — so a function used only from integration tests is not dead.

## Dead types and constants

`DRY-006` asks the same question of everything that is not a function: a
`struct`, `enum`, `union`, type alias, `const` or `static` that nothing in the
workspace reaches. References are counted across macro bodies and attribute
arguments too, and a type's own `impl` blocks deliberately do not count —
carrying methods keeps nothing alive, which is the same verdict rustc reaches
with "never constructed".

*Reaches*, not *mentions*: being named is only evidence if whoever named it is
reached in turn. Two types that refer to each other, a ring of three, or the
common shape where the mutual references sit in `impl` methods — none of them
keeps anything alive without an entry point from outside the group. The walk
starts at references made from code that is not itself a candidate (a function
body, a trait, a `use`), and at declarations that are excused anyway
(`#[allow(dead_code)]`, `qual:api`, `qual:test_helper`), so what those name keeps
its user — one marker on the entry point covers the cluster behind it. Being
`pub` is deliberately not an entry point: a public type nobody in the workspace
uses is exactly the finding, and a library whose consumers live elsewhere says
so with `// qual:api`.

The value over rustc's own `dead_code` lint is the crate boundary: a `pub` type
nobody in the workspace uses is invisible to the compiler. Deliberate
exceptions take `#[allow(dead_code)]`, and a leading underscore (`_Marker`) is
honoured exactly as rustc honours it. Exports — `#[no_mangle]`, `#[used]`,
`#[export_name]`, in either spelling — are exempt without any marker, in both
dead-code checks: their caller is a linker, so having no call site in the
workspace is the normal state.

Traits are out of scope: every `impl Trait for X` names the trait, so a trait
with one implementation would always look used.

Two limits worth knowing. A file rustqual cannot parse is dropped from the
analysis, so anything used only there reads as unused — that affects every
dimension, but types are numerous enough that it shows up here first, so check
stderr for parse warnings before acting on a surprising batch of findings. And
code generated at build time (`include!(concat!(env!("OUT_DIR"), …))`) is not in
the `.rs` set at all; mark what it consumes with `// qual:api`.

## Code fragments and repeated matches

`DRY-003` finds ≥6-line blocks repeated across functions — usually setup boilerplate that should be a helper, or assertion patterns that should be a test utility.

`DRY-005` finds repeated `match` blocks: identical arms, ≥3 of them, in ≥3 different functions. Classic case is dispatching the same enum to slightly different methods in five places — extract a helper.

## Wildcard imports

`DRY-004` flags every `use foo::*;`. Wildcards hide:

- Which symbols you actually depend on.
- Layer-tunneling (a wildcard re-export can pull in domain types into adapters without it being visible).
- Name collisions when the upstream module adds new symbols.

Replace with explicit imports. If a `prelude::*` is unavoidable (some crates require it), suppress narrowly:

```rust
// qual:allow(dry, wildcard_imports) reason: "diesel requires this prelude for query DSL"
use diesel::prelude::*;
```

## Boilerplate

The `BP-*` rules detect patterns where the compiler or a derive macro could write the code for you. Each finding includes a suggested replacement. Examples:

- `BP-001` `impl From<A> for B { fn from(a: A) -> B { B { x: a.x, y: a.y } } }` → `#[derive(...)]` or struct shorthand.
- `BP-005` Manual `impl Default` that just calls every field's default → `#[derive(Default)]`.
- `BP-007` Hand-written error enum with `From` impls and `Display` mapping → `#[derive(thiserror::Error)]`.

These rarely require thinking — just apply the suggestion and move on. Disable the boilerplate dimension if your project has a reason to avoid derive macros:

```toml
[boilerplate]
enabled = false
```

## Configure thresholds

```toml
[duplicates]
enabled = true
# similarity_threshold = 0.95
# min_function_lines = 6

[boilerplate]
enabled = true
```

Most thresholds are tuned to be opinionated by default. Loosen them via `--init` if you want them calibrated to your current codebase metrics.

## What you'll see

```
✗ DRY-001  src/api/users.rs::format_user (line 88)
            duplicate of src/api/orders.rs::format_order (96% similarity)

✗ DRY-002  src/utils/legacy.rs::old_helper (line 12) — dead code

⚠ DRY-004  src/api/handlers.rs uses `use db::*;` (line 4)

✗ BP-001   src/error.rs impl From<io::Error> for AppError — derivable
```

## Suppression

For genuine cases where suppression is right:

```rust
// qual:allow(dry, duplicate) reason: "keeping this duplicate temporarily; consolidating in PR-345"
fn old_path() { /* … */ }

// qual:api — public-API entry, callers outside this crate
pub fn parse(input: &str) -> Result<Ast> { /* … */ }

// qual:test_helper — used only from integration tests
pub fn build_test_config() -> Config { /* … */ }

// qual:inverse(decode)
fn encode(v: &Value) -> Vec<u8> { /* … */ }
```

`qual:api`, `qual:test_helper`, and `qual:inverse` don't count against `max_suppression_ratio`. `qual:allow(dry, …)` does.

Full annotation reference: [reference-suppression.md](./reference-suppression.md).

## Why this matters for AI-generated code

AI agents are particularly prone to copy-paste-with-variation: when asked to "do the same for X", they tend to copy the existing implementation rather than extract a shared abstraction. `DRY-001` and `DRY-003` catch that mechanically. After a few cycles of `rustqual` flagging duplicates, the agent learns to extract; the codebase stays denormalised.

`DRY-002` is the other half of that loop — agents sometimes generate helpers that go unused because they pivoted mid-task. Catching dead code at PR time prevents accumulation.

## Related

- [function-quality.md](./function-quality.md) — IOSP, complexity (where duplication often lives)
- [test-quality.md](./test-quality.md) — `TQ-003` (untested) shares the call graph with `DRY-002`
- [reference-rules.md](./reference-rules.md) — every rule code with details
- [reference-suppression.md](./reference-suppression.md) — `qual:api`, `qual:test_helper`, `qual:inverse`
