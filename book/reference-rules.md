# Reference: rule catalog

Every rule rustqual emits, grouped by dimension. Codes are stable
identifiers — but where they actually surface differs by reporter:

- **SARIF**: every result carries the catalog code as `ruleId` and is
  registered in `tool.driver.rules` (architecture also registers
  dynamic sub-IDs like `architecture/call_parity/no_delegation`).
- **JSON**: findings are grouped by dimension section (`duplicates`,
  `dead_code`, `tq_warnings`, `architecture_findings`, …) — pivot on
  the section, not on a `code` field. Only `architecture_findings`
  rows carry their full `rule_id`.
- **GitHub** workflow-command annotations: emitted as
  `::level file=...,line=...::message`. Catalog codes are not
  written to a structured `title` property today; if you need them
  in CI for filtering, use the SARIF format with Code Scanning.
- **Text** / **findings-list**: the category column shows
  human-readable labels (`COGNITIVE`, `DUPLICATE`, `ARCHITECTURE`,
  `BOILERPLATE`, …), not the catalog codes.
- **`// qual:allow`** rationales: cite the catalog code in the
  `reason:` text — it is the canonical reference users grep for.

For dimension intent and refactor patterns, see the use-case guides linked at the bottom.

## IOSP

| Code | Meaning |
|---|---|
| `iosp/violation` | Function mixes orchestration with logic — split into Integration + Operation(s). |

## Complexity (CX-*)

| Code | Meaning | Default threshold |
|---|---|---|
| `CX-001` | Cognitive complexity exceeds threshold | ≤ 15 |
| `CX-002` | Cyclomatic complexity exceeds threshold | ≤ 10 |
| `CX-003` | Magic-number literal in non-const context | (any literal not in `const`/`static`) |
| `CX-004` | Function length exceeds threshold | ≤ 60 lines |
| `CX-005` | Nesting depth exceeds threshold | ≤ 4 |
| `CX-006` | Unsafe block detected | `detect_unsafe = true` |
| `A20`    | Error-handling issue (`unwrap`/`expect`/`panic!`/`todo!`) | `detect_error_handling = true` |

`A20` and `CX-004` skip `#[test]` functions and workspace-root `tests/**` files.

## DRY

| Code | Meaning |
|---|---|
| `DRY-001` | Duplicate function (95%+ token similarity) |
| `DRY-002` | Dead code — function defined but never called |
| `DRY-003` | Duplicate code fragment (≥6 lines repeated) |
| `DRY-004` | Wildcard import (`use foo::*;`) |
| `DRY-005` | Repeated match pattern across functions (≥3 arms, ≥3 instances) |

## Boilerplate (BP-*)

| Code | Meaning |
|---|---|
| `BP-001` | Trivial `From` impl (derivable) |
| `BP-002` | Trivial `Display` impl (derivable) |
| `BP-003` | Trivial getter/setter (consider field visibility) |
| `BP-004` | Builder pattern (consider derive macro) |
| `BP-005` | Manual `Default` impl (derivable) |
| `BP-006` | Repetitive match mapping |
| `BP-007` | Error enum boilerplate (consider `thiserror`) |
| `BP-008` | Clone-heavy conversion |
| `BP-009` | Struct-update boilerplate |
| `BP-010` | Format-string repetition |

## SRP

| Code | Meaning |
|---|---|
| `SRP-001` | Struct may violate Single Responsibility Principle (composite: fields + methods + cohesion) |
| `SRP-002` | Module file too long (default warn 300, hard 800 production lines) |
| `SRP-003` | Function has too many parameters (default > 5) |

## Coupling

| Code | Meaning |
|---|---|
| `CP-001` | Circular module dependency |
| `CP-002` | Stable Dependencies Principle violation |
| `CP-003` | Module instability exceeds configured threshold |

## Structural binary checks

Part of SRP (BTC, SLM, NMS) and Coupling (OI, SIT, DEH, IET).

| Code | Meaning |
|---|---|
| `BTC` | Broken trait contract — every method in an `impl Trait` block is a stub |
| `SLM` | Selfless method — takes `self` but never references it |
| `NMS` | Needless `&mut self` — declares mutable receiver but never mutates |
| `OI`  | Orphaned impl — `impl Foo` block in different file from `struct Foo` |
| `SIT` | Single-impl trait — non-`pub` trait with exactly one implementation |
| `DEH` | Downcast escape hatch — use of `Any::downcast` |
| `IET` | Inconsistent error types within a module |

## Test Quality (TQ-*)

| Code | Meaning |
|---|---|
| `TQ-001` | Test function has no assertions |
| `TQ-002` | Test function does not call any production function |
| `TQ-003` | Production function is untested (no test calls it) |
| `TQ-004` | Production function has no coverage (LCOV-based, requires `--coverage`) |
| `TQ-005` | Untested logic branches — covered function with uncovered lines |

## Architecture

Architecture findings emit hierarchical rule IDs of the form
`architecture/<rule-family>[/<sub-kind>]`. The `<sub-kind>` is dynamic
for pattern and trait-contract rules (the user-defined rule's `name` /
`check` string).

| Rule ID | Meaning |
|---|---|
| `architecture/layer` | Layer rule violation — file imports outside its allowed direction |
| `architecture/layer/unmatched` | File doesn't match any configured layer glob (under `unmatched_behavior = "strict_error"`) |
| `architecture/forbidden` | Forbidden-edge violation — `[[architecture.forbidden]]` rule fired |
| `architecture/pattern/<name>` | Symbol-pattern violation — `[[architecture.pattern]]` rule with the given `name` fired (e.g. `architecture/pattern/no_panic_helpers_in_production`) |
| `architecture/trait_contract` | Trait-contract violation — generic catch-all |
| `architecture/trait_contract/<check>` | Trait-contract violation with a specific `<check>` kind (e.g. `architecture/trait_contract/object_safety`) |
| `architecture/call_parity/no_delegation`        | Check A — adapter `pub fn` doesn't reach the target layer at all |
| `architecture/call_parity/missing_adapter`      | Check B — target `pub fn` is in some adapter's coverage but missing from another (or transitively unreachable from any adapter touchpoint — orphan / dead island) |
| `architecture/call_parity/multi_touchpoint`     | Check C — adapter `pub fn` has more than one touchpoint in the target layer (configurable severity via `single_touchpoint`, default `warn`) |
| `architecture/call_parity/multiplicity_mismatch` | Check D — target `pub fn` is reached by every adapter but with divergent handler counts (e.g. cli=2, mcp=1) |

## Suppression / governance

| Code | Meaning |
|---|---|
| `SUP-001`    | Suppression ratio exceeds configured maximum (default 5%). Warn by default; error with `--fail-on-warnings`. |
| `ORPHAN-001` | Stale `qual:allow` marker — no finding in the annotation window. |

## Severity & default-fail

By default, every finding fails the build (exit code `1`). Override with `--no-fail` for local exploration, or `--min-quality-score <N>` to allow some findings as long as the overall score holds.

Warnings (`SUP-001`) don't fail by default — pass `--fail-on-warnings` to flip that.

## Related

- [reference-configuration.md](./reference-configuration.md) — every config option in `rustqual.toml`
- [reference-suppression.md](./reference-suppression.md) — `qual:allow`, `qual:api`, etc.
- [function-quality.md](./function-quality.md) — IOSP, CX, A20
- [module-quality.md](./module-quality.md) — SRP-*
- [coupling-quality.md](./coupling-quality.md) — CP-*, OI, SIT, DEH, IET
- [code-reuse.md](./code-reuse.md) — DRY-*, BP-*
- [test-quality.md](./test-quality.md) — TQ-*
- [architecture-rules.md](./architecture-rules.md) — ARCH-*
- [adapter-parity.md](./adapter-parity.md) — `architecture/call_parity/*`
