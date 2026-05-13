# Use case: coupling

Coupling is what makes a refactor cost a week instead of an hour. Two modules that import each other can't be tested independently, can't be deployed independently, and tend to drift toward a single mega-module over time. rustqual measures coupling at the module level and flags the patterns that historically lead to architecture decay.

## What goes wrong

- **Circular dependencies** — `a` imports `b`, `b` imports `a`. Either directly or through a chain. Fundamentally breaks layering.
- **Unstable cores** — modules that are heavily depended on (high fan-in) but also depend on a lot of other modules (high fan-out). Every change ripples; nothing is safe to touch.
- **Stable Dependencies Principle violations** — a stable module (lots of incoming deps) depends on an unstable one (few incoming, many outgoing). Should be the other way around.
- **Orphaned impl blocks** — `impl Foo` lives in a different file than `struct Foo`. Useful occasionally; usually a "I'll move it later" smell.
- **Single-impl traits** — a non-public trait with exactly one impl. The trait isn't an abstraction, it's a stub waiting to be inlined.
- **Downcast escape hatches** — `Any::downcast`/`downcast_ref`. Almost always means a missing enum or trait method.
- **Inconsistent error types** — one module returns `Result<_, MyError>` from half its functions and `Result<_, anyhow::Error>` from the other half.

## What rustqual catches

| Rule | Meaning |
|---|---|
| `CP-001` | Circular module dependency |
| `CP-002` | Stable Dependencies Principle violation — a stable module depends on a less stable one |
| `OI`     | Orphaned impl: `impl Foo` block in a different file from `struct Foo` |
| `SIT`    | Single-impl trait: non-`pub` trait with exactly one implementation |
| `DEH`    | Downcast escape hatch: use of `Any::downcast` |
| `IET`    | Inconsistent error types within a module |

`OI`, `SIT`, `DEH`, `IET` are part of the **structural binary checks** under the Coupling dimension. They each fire as a single binary signal per module, which is why they don't have numeric thresholds.

### Instability metric

For each module, rustqual computes `I = fan_out / (fan_in + fan_out)`:

- `I = 0` — purely depended-upon, depends on nothing (stable: domain types).
- `I = 1` — depends on everything, nothing depends on it (unstable: top-level orchestration).

The Stable Dependencies Principle says *dependencies should flow toward stability* — stable modules at the bottom, unstable ones at the top. `CP-002` fires when a stable module imports an unstable one, which inverts the dependency direction.

## Configure thresholds

```toml
[coupling]
enabled = true
max_instability = 0.8     # warn above this
max_fan_in = 15
max_fan_out = 12
check_sdp = true          # disable SDP check if you don't want it

[structural]
enabled = true
check_oi = true
check_sit = true
check_deh = true
check_iet = true
```

`--init` calibrates `max_fan_in` and `max_fan_out` to your current codebase metrics.

## What you'll see

```
✗ CP-001  src/auth/session.rs ↔ src/auth/token.rs (cycle of length 2)
✗ CP-002  src/domain/user.rs depends on src/api/handlers.rs (I=0.91)
⚠ OI      impl Order in src/orders/persistence.rs (struct in src/orders/types.rs)
⚠ SIT     trait OrderValidator (1 impl: BasicOrderValidator)
⚠ DEH     downcast in src/registry/lookup.rs (line 88)
⚠ IET     src/payment/mod.rs uses MyError (4×) and anyhow::Error (3×)
```

`--format dot` produces a Graphviz **per-function IOSP call graph** (nodes are functions, coloured by classification; edges follow `own_calls`). For module-level cycles and dependency direction, use the `coupling` table in the text or HTML report instead.

## Refactor patterns

**Cycle**: usually one of the two modules has a function that belongs in the other. Move the function and the cycle dissolves. If both modules genuinely need each other, extract the shared types into a third module they both depend on.

**SDP violation**: invert the dependency — typically by introducing a trait in the stable module that the unstable one implements. The stable module then knows nothing about the unstable one.

**Orphaned impl**: move the `impl` block next to the type definition, or — if it's a trait impl that *can't* live there (orphan rules) — accept it and add `// qual:allow(coupling)`.

**Single-impl trait**: inline the trait. Most "interface for testability" cases can be replaced by direct dependency injection of the concrete type, or by a trait that has more than one real impl somewhere in the codebase.

**Downcast**: replace with an enum. If you find yourself reaching for `Any::downcast`, the type system is telling you the cases were never enumerated.

**Inconsistent errors**: pick one. Either every public function in the module returns `MyError` (with `From` impls for upstream errors), or every public function returns `anyhow::Error`. Don't mix.

## Suppression

Coupling warnings are *module-level*, not function-level. The `qual:allow(coupling)` annotation on a single function doesn't silence them — that's intentional. To suppress a coupling finding for a whole module:

```rust
// At the top of the module file:
//! qual:allow(coupling) — orchestration layer, intentionally depends on every adapter.
```

Inner doc-comment form (`//!`) attaches to the module, not to a single item.

For structural-binary checks (`OI`, `SIT`, `DEH`, `IET`) which target specific items, use `// qual:allow(coupling)` at the impl/trait/use site.

## Related

- [architecture-rules.md](./architecture-rules.md) — explicit layer rules instead of metrics
- [module-quality.md](./module-quality.md) — within-module structure (SRP)
- [reference-rules.md](./reference-rules.md) — every rule code with details
- [reference-configuration.md](./reference-configuration.md) — every config option
