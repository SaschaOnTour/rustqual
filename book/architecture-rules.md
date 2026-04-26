# Use case: architecture rules

The Architecture dimension is rustqual's "I want this codebase to look like *that* in five years" enforcement layer. Where IOSP, complexity, and SRP catch *local* smells, architecture rules catch the *global* drift — the kind that turns a clean hexagonal design into a tangle of cross-imports over six months.

Architecture rules are config-driven. You write them once in `rustqual.toml`, and they apply on every analysis run. If a PR violates a rule, CI fails. There's no "I'll fix it later" — the rule is the source of truth.

## What you can enforce

- **Layers** — domain → ports → infrastructure → application. Inner layers can't import outer ones.
- **Forbidden edges** — `analyzer X` cannot import `analyzer Y`, regardless of layer. Specific cross-cuts.
- **Symbol patterns** — where specific paths/macros/methods are allowed (e.g., `syn::` only in adapters, `unwrap()` only in tests, `println!` only in CLI).
- **Trait contracts** — every port trait must be object-safe, `Send + Sync`, and not leak adapter error types.

These are independent rule families. You can use any combination. Most projects start with layers, add forbidden edges as they discover specific cross-cuts to forbid, and add symbol patterns when a particular antipattern keeps coming back.

## Layers

The defining structural rule. Inner layers know nothing of outer ones; outer layers can use inner ones freely.

```toml
[architecture.layers]
order = ["domain", "ports", "infrastructure", "analysis", "application"]
unmatched_behavior = "strict_error"   # files outside any layer fail the dim

[architecture.layers.domain]
paths = ["src/domain/**"]

[architecture.layers.ports]
paths = ["src/ports/**"]

[architecture.layers.infrastructure]
paths = ["src/adapters/config/**", "src/adapters/source/**"]

[architecture.layers.analysis]
paths = ["src/adapters/analyzers/**", "src/adapters/report/**"]

[architecture.layers.application]
paths = ["src/app/**"]
```

A `domain` file importing from `application` fails. An `application` file importing from `domain` is fine. Same-layer imports are always fine.

### `unmatched_behavior`

Three options:

- `"strict_error"` — every production file must match a layer (hard finding otherwise). Recommended; flags new files dropped in arbitrary locations.
- `"composition_root"` — unmatched files act as the composition root and may import any layer.
- `"warn"` — soft warning instead of error.

### Re-export points

Some files (`lib.rs`, `main.rs`, `bin/**`, `cli/**`, `tests/**`) live at the root and re-export from every layer. Mark them explicitly:

```toml
[architecture.reexport_points]
paths = ["src/lib.rs", "src/main.rs", "src/bin/**", "src/cli/**", "tests/**"]
```

### Workspaces

For multi-crate workspaces, map external crates to layers:

```toml
[architecture.external_crates]
my_domain_types = "domain"
my_port_traits  = "ports"
```

Now `infrastructure` can import `my_domain_types` (lower-rank, allowed) but not the other way around.

## Forbidden edges

Where layers are too coarse, forbidden edges name specific source/destination pairs:

```toml
[[architecture.forbidden]]
from = "src/adapters/analyzers/iosp/**"
to   = "src/adapters/analyzers/**"
except = ["src/adapters/analyzers/iosp/**"]
reason = "Dimension analyzers don't know each other"

[[architecture.forbidden]]
from = "src/adapters/**"
to   = "src/app/**"
reason = "Adapters know domain + ports, not application"
```

Each rule has `from`, `to`, an optional `except` list, and a human-readable `reason` that shows up in the finding.

## Symbol patterns

The most flexible family — you can forbid specific path prefixes, method calls, macro calls, or glob imports in specific directories:

```toml
# AST types only in adapters
[[architecture.pattern]]
name = "no_syn_in_domain"
forbid_path_prefix = ["syn::", "proc_macro2::", "quote::"]
forbidden_in = ["src/domain/**"]
reason = "Domain has no AST representation"

# unwrap()/expect() only in tests
[[architecture.pattern]]
name = "no_panic_helpers_in_production"
forbid_method_call = ["unwrap", "expect"]
forbidden_in = ["src/**"]
except = ["**/tests/**"]
reason = "Production propagates errors typed instead of panicking"

# println!/print!/dbg! only in CLI/binaries
[[architecture.pattern]]
name = "no_stdout_in_library_code"
forbid_macro_call = ["println", "print", "dbg"]
forbidden_in = ["src/**"]
allowed_in = ["src/main.rs", "src/bin/**", "src/cli/**"]
reason = "stdout is the CLI's channel, not library code's"

# No glob imports in domain
[[architecture.pattern]]
name = "no_glob_imports_in_domain"
forbid_glob_import = true
forbidden_in = ["src/domain/**"]
reason = "Glob imports hide layer tunneling"
```

Each rule carries `forbidden_in` (where it fires) and optional `allowed_in`/`except` (where it's exempted). The `reason` field is mandatory and shows up in the finding so reviewers know *why*.

## Trait contracts

The most prescriptive family — used to keep port traits clean:

```toml
[[architecture.trait_contract]]
name = "port_traits"
scope = "src/ports/**"

receiver_may_be = ["shared_ref"]              # only &self, no &mut self / self
forbidden_return_type_contains = [
    "anyhow::", "Box<dyn",                     # no untyped errors, no boxed dyns
]
forbidden_error_variant_contains = [
    "syn::", "toml::", "serde_json::",        # adapter errors don't leak
]
must_be_object_safe = true                     # for dyn dispatch
required_supertraits_contain = ["Send", "Sync"]
```

This catches a port that someone "almost" got right — for example, a port trait that accidentally exposes `&mut self` or returns `anyhow::Result<…>` instead of a typed error.

Plus the structural binary check `BTC` (broken trait contract) flags impls that are entirely stubs (`unimplemented!`, `todo!`, `Default::default()` only).

## What you'll see

```
✗ ARCH-LAYER  src/domain/order.rs imports src/adapters/source/io.rs
              domain (rank 0) cannot import infrastructure (rank 2)

✗ ARCH-FORBID src/adapters/analyzers/iosp/visitor.rs imports
              src/adapters/analyzers/dry/mod.rs
              reason: Dimension analyzers don't know each other

✗ ARCH-PATTERN src/auth/session.rs uses unwrap() (line 88)
              rule: no_panic_helpers_in_production
              reason: Production propagates errors typed instead of panicking

✗ ARCH-TRAIT  src/ports/storage.rs trait Storage::write returns anyhow::Result
              rule: port_traits
              reason: forbidden_return_type_contains: anyhow::
```

## Diagnostic mode

`rustqual --explain src/some/file.rs` prints which layer the file matches, which symbol rules apply, and what would change if you moved it. Useful when you can't tell why a file is failing.

## Configure

```toml
[architecture]
enabled = true
# Then add layers, forbidden edges, patterns, trait_contract sections
```

`--init` doesn't generate architecture rules — they require an opinion about your design that the tool can't infer. Add them manually, ratchet up over time.

## Suppression

Architecture is suppression-resistant by design. The `// qual:allow(architecture)` annotation works at the import site or item, but it counts hard against `max_suppression_ratio`, and you should leave a `reason:` rationale in the comment block:

```rust
// qual:allow(architecture) — port adapter must call into the registry directly
// here for serialization round-trip; pure domain accessor would lose ordering.
use crate::adapters::registry::lookup;
```

The right answer is usually to widen the rule (with a clear `except` clause) or move the file, not to suppress.

## Why this is unusual

Most static analyzers ship per-function rules and stop there. Architecture linters (ArchUnit, dependency-cruiser) prove what *can't* be called. rustqual's architecture dimension does both directions:

- **Negative space** (forbidden edges, layer rules, symbol patterns) — what mustn't happen.
- **Positive space** (call parity — see [adapter-parity.md](./adapter-parity.md)) — what *must* happen across multiple adapters.

The combination is what makes drift mechanically detectable rather than review-dependent.

## Related

- [adapter-parity.md](./adapter-parity.md) — call parity, the architecture rule that's unique to rustqual
- [coupling-quality.md](./coupling-quality.md) — metric-based coupling (instability, SDP)
- [reference-rules.md](./reference-rules.md) — every rule code with details
- [reference-configuration.md](./reference-configuration.md) — every config option
