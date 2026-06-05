# Reference: configuration

`rustqual.toml` lives next to `Cargo.toml` and configures every dimension. Generate a starter file calibrated to your codebase:

```bash
rustqual --init
```

Below is the full schema, grouped by section. Every field has a default; a minimal config can omit anything you don't want to tune.

## Top-level

| Key | Default | Meaning |
|---|---|---|
| `exclude_files` | `[]` | Glob patterns for files to skip entirely |
| `strict_closures` | `false` | Treat closures as logic (stricter IOSP) |
| `strict_iterator_chains` | `false` | Treat `.map`/`.filter`/`.fold` as logic |
| `allow_recursion` | `false` | Allow self-calls without counting as IOSP violation |
| `strict_error_propagation` | `false` | Count `?` as logic |
| `max_suppression_ratio` | `0.05` | Cap on `qual:allow` annotations as fraction of functions |

```toml
exclude_files = ["examples/**", "vendor/**"]
max_suppression_ratio = 0.05
```

> **Removed in 1.5.0:** the `ignore_functions` option no longer exists. It
> excluded matching functions from *every* dimension — too blunt — and a
> `rustqual.toml` that still sets it will fail to parse. To exempt code, use
> `// qual:allow(<dim>)`, `// qual:api`, `// qual:test_helper`, or
> `exclude_files`. (`visit_*` methods no longer need exempting: TQ-003 models
> syn-visitor dispatch directly, and SRP cohesion accounts for trait methods.)

## `[complexity]`

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Enable the dimension |
| `max_cognitive` | `15` | `CX-001` threshold |
| `max_cyclomatic` | `10` | `CX-002` threshold |
| `max_nesting_depth` | `4` | `CX-005` threshold |
| `max_function_lines` | `60` | `CX-004` threshold |
| `detect_unsafe` | `true` | Emit `CX-006` on `unsafe` blocks |
| `detect_error_handling` | `true` | Emit `A20` on `unwrap`/`expect`/`panic!`/`todo!` |
| `allow_expect` | `false` | Permit `.expect()` while still flagging `.unwrap()` |

## `[duplicates]`

```toml
[duplicates]
enabled = true
```

`DRY-001` similarity threshold (95% by default) and `DRY-003` minimum-fragment-length (6 lines) are currently fixed.

## `[boilerplate]`

```toml
[boilerplate]
enabled = true
```

The full `BP-*` family. Disable if your project deliberately avoids derive macros.

## `[srp]`

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Enable the dimension |
| `smell_threshold` | `0.6` | Composite score threshold for `SRP-001` |
| `max_fields` | `12` | Field count over which `SRP-001` weighs more |
| `max_methods` | `20` | Method count over which `SRP-001` weighs more |
| `max_fan_out` | `10` | Per-struct fan-out bound |
| `max_parameters` | `5` | `SRP-003` threshold |
| `lcom4_threshold` | `2` | Number of disjoint clusters before LCOM4 contributes |
| `file_length_baseline` | `300` | Soft warn for `SRP-002` (production lines) |
| `file_length_ceiling` | `800` | Hard finding for `SRP-002` |
| `max_independent_clusters` | `2` | Max disjoint cluster count |
| `min_cluster_statements` | `5` | Minimum statements for a cluster to count |

## `[coupling]`

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Enable the dimension |
| `max_instability` | `0.8` | Warn when module instability exceeds this |
| `max_fan_in` | `15` | Per-module fan-in bound |
| `max_fan_out` | `12` | Per-module fan-out bound |
| `check_sdp` | `true` | Stable Dependencies Principle (`CP-002`) |

## `[structural]`

Binary checks: BTC, SLM, NMS, OI, SIT, DEH, IET.

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Enable the dimension |
| `check_btc` | `true` | Broken trait contract |
| `check_slm` | `true` | Selfless method |
| `check_nms` | `true` | Needless `&mut self` |
| `check_oi` | `true` | Orphaned impl |
| `check_sit` | `true` | Single-impl trait |
| `check_deh` | `true` | Downcast escape hatch |
| `check_iet` | `true` | Inconsistent error types |

## `[test_quality]`

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Enable the dimension |
| `extra_assertion_macros` | `[]` | Custom macro names to recognise as assertions in `TQ-001` |

```toml
[test_quality]
extra_assertion_macros = ["verify", "check_invariant", "expect_that"]
```

`TQ-004` and `TQ-005` activate when `--coverage <LCOV_FILE>` is supplied.

## `[tests]`

Per-test-code threshold overrides. rustqual applies a **fixed, curated**
subset of checks to test code — `DRY-001`/`DRY-004`/`DRY-005`, `LONG_FN`
(function length), and SRP **file-length** (`SRP_MODULE`). The rest stay
test-exempt (`ERROR_HANDLING`, `MAGIC_NUMBER`, IOSP, `DRY-002` dead code,
`DRY-003` wildcard imports, Coupling, all Structural detectors). The SRP
**module-cohesion** check (independent function clusters) is also
**production-only**: a test file's many independent `#[test]` fns are its
purpose, not a low-cohesion smell. The **god-struct** check (`SRP-001`) *does*
fire on test structs — a god-fixture wiring up many concerns is a real smell —
but at production thresholds, with no separate test knob (use `// qual:allow(srp)`
for the rare legitimate fixture). Which checks apply is **not** configurable;
only the thresholds below are.

Each key overrides the matching production threshold *for test code only*.
An unset key inherits the production value, so by default test code is judged
by the same limits as production. Field names mirror `[complexity]` and
`[srp]`.

| Key | Inherits | Meaning |
|---|---|---|
| `max_function_lines` | `[complexity].max_function_lines` | `LONG_FN` limit for test fns |
| `file_length_baseline` | `[srp].file_length_baseline` | File-length score baseline for test files |
| `file_length_ceiling` | `[srp].file_length_ceiling` | File-length score ceiling for test files |

```toml
[tests]
max_function_lines   = 120
file_length_baseline = 500
file_length_ceiling  = 1200
```

## `[weights]`

Quality-score weights. Must sum to `1.0`.

```toml
[weights]
iosp         = 0.22
complexity   = 0.18
dry          = 0.13
srp          = 0.18
coupling     = 0.09
test_quality = 0.10
architecture = 0.10
```

## `[architecture]`

The largest section. Top-level toggle:

```toml
[architecture]
enabled = true
```

Then one or more rule families:

### `[architecture.layers]`

```toml
[architecture.layers]
order = ["domain", "ports", "infrastructure", "analysis", "application"]
unmatched_behavior = "strict_error"   # or "composition_root", "warn"

[architecture.layers.domain]
paths = ["src/domain/**"]

[architecture.layers.application]
paths = ["src/app/**"]
```

`unmatched_behavior` controls files outside any layer:

- `"strict_error"` — fail (recommended).
- `"composition_root"` — treat as the root that may import any layer.
- `"warn"` — soft warning.

### `[architecture.reexport_points]`

```toml
[architecture.reexport_points]
paths = ["src/lib.rs", "src/main.rs", "src/bin/**", "src/cli/**", "tests/**"]
```

Files marked here bypass the layer rule.

### `[architecture.external_crates]`

For multi-crate workspaces:

```toml
[architecture.external_crates]
my_domain_types = "domain"
my_port_traits  = "ports"
```

### `[[architecture.forbidden]]`

Repeatable. Per-rule fields: `from`, `to`, `except` (optional), `reason`.

```toml
[[architecture.forbidden]]
from = "src/adapters/**"
to   = "src/app/**"
reason = "Adapters know domain + ports, not application"
```

### `[[architecture.pattern]]`

Repeatable symbol-pattern rules.

| Field | Meaning |
|---|---|
| `name` | Identifier shown in findings |
| `forbid_path_prefix` | List of `crate::` / `module::` prefixes to forbid |
| `forbid_method_call` | List of method names to forbid (`unwrap`, `expect`, …) |
| `forbid_macro_call` | List of macro names to forbid (`println`, `dbg`, …) |
| `forbid_glob_import` | `true` to forbid `use foo::*;` |
| `forbidden_in` | Globs where the rule fires |
| `allowed_in` | Globs where the rule is exempted |
| `except` | Globs in `forbidden_in` that are exempted |
| `reason` | Mandatory rationale |

### `[[architecture.trait_contract]]`

Repeatable trait-shape rules.

| Field | Meaning |
|---|---|
| `name` | Identifier shown in findings |
| `scope` | Glob of files where the rule applies |
| `receiver_may_be` | Allowed receiver kinds: `"shared_ref"`, `"mut_ref"`, `"owned"` |
| `forbidden_return_type_contains` | Substrings forbidden in return types |
| `forbidden_error_variant_contains` | Substrings forbidden in error types (`Result<_, E>`) |
| `must_be_object_safe` | `true` to require object-safety |
| `required_supertraits_contain` | Required supertrait substrings (`"Send"`, `"Sync"`) |

### `[architecture.call_parity]`

Single-instance section.

| Field | Default | Meaning |
|---|---|---|
| `adapters` | (required) | List of adapter layer names |
| `target` | (required) | Target layer name |
| `call_depth` | `3` | Adapter-internal traversal depth — max call edges the boundary BFS follows from the adapter pub-fn (direct callees at depth 1, so `3` reaches `handler → h1 → h2 → target`). Does not constrain post-boundary application chain depth. |
| `exclude_targets` | `[]` | Globs (module-path form) to skip from Check B |
| `transparent_wrappers` | `[]` | Wrapper type names to peel during receiver-type inference |
| `transparent_macros` | (default list) | Attribute macros treated as transparent |
| `promoted_attributes` | `[]` | Bare attribute names that lift a private fn onto the adapter-handler surface (rmcp `#[tool]`, axum/poem `#[handler]`, etc.) |
| `single_touchpoint` | `"warn"` | Check C severity: `"off"` skips, `"warn"` emits as `Severity::Low`, `"error"` as `Severity::Medium` |

```toml
[architecture.call_parity]
adapters = ["cli", "mcp"]
target   = "application"
call_depth = 3
exclude_targets = ["application::admin::*"]
transparent_wrappers = ["State", "Extension", "Json", "Data"]
promoted_attributes = ["tool"]   # rmcp #[tool] private async fns count as handlers
single_touchpoint = "warn"
```

### `promoted_attributes` — proc-macro-generated handlers

rustqual reads pre-expansion source. Frameworks that generate the
public dispatch surface at macro-expansion time (rmcp's
`#[tool_router]` / `#[tool]`, axum's `#[handler]`, poem's
`#[handler]`, …) leave only a syntactically *private* user-written
fn behind. Without this knob, every application call those methods
make appears as "not reached from adapter X" because they don't
qualify as adapter handlers.

Configure with the bare attribute name (`"tool"` not
`"rmcp::tool"`). When a private impl method or free fn carries any
attribute whose path-leaf matches an entry, it's treated as part of
the adapter surface for Checks A/B/C/D.

When a finding fires *and* a private fn in the missing adapter
carries a non-stdlib attribute that transitively reaches the
unreached target, the finding's message includes a hint pointing
at the candidate so the author knows immediately which attribute to
add:

```
src/application/session.rs:5  ARCHITECTURE  '...session::open' is not reached from adapter layer(s): mcp: call parity
hint: 1 private fn in mcp transitively reaches this target:
  - src/mcp/server.rs:8 search has #[tool] attribute(s)
Add the attribute name to `[architecture.call_parity] promoted_attributes` if it marks a macro-generated handler entry point.
```

The hint reuses the production `compute_touchpoints` walker to
verify reachability — `call_depth`, peer-adapter, and
boundary-stop rules are applied identically to the actual Check B
walk. A candidate appears in the hint iff promoting it would
actually put the target into the adapter's coverage. No parallel
reachability logic to maintain, no false leads.

**Deprecated handlers** (`#[deprecated]` on adapter `pub fn`s) are
excluded from Checks A/B/C/D automatically — no config knob.

## `[report]`

Workspace-mode aggregation:

```toml
[report]
aggregation = "loc_weighted"
```

Aggregation strategies: `"loc_weighted"` (default), `"unweighted"`.

## Composition

Most projects converge on a layout like:

```toml
exclude_files = ["examples/**"]
max_suppression_ratio = 0.05

[complexity]
enabled = true

[duplicates]
enabled = true

[srp]
enabled = true

[coupling]
enabled = true

[test_quality]
enabled = true

[architecture]
enabled = true

[architecture.layers]
order = ["domain", "ports", "infrastructure", "application"]
unmatched_behavior = "strict_error"

[architecture.layers.domain]
paths = ["src/domain/**"]
# … etc.
```

Use `--init` to bootstrap with calibrated thresholds, then trim.

## Related

- [reference-cli.md](./reference-cli.md) — flags that override or supplement config
- [reference-rules.md](./reference-rules.md) — every rule that config keys gate
- [reference-suppression.md](./reference-suppression.md) — `qual:allow` etc.
- [getting-started.md](./getting-started.md) — `--init` and first-run workflow
