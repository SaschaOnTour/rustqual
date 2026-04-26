# Use case: adapter parity (call parity)

If your project has multiple frontends — a CLI, an MCP server, a REST API, a web UI — they're supposed to expose the same underlying capabilities. In theory, every CLI command has a matching MCP handler. In practice, it drifts. Someone adds a new application function, MCP picks it up, CLI forgets, and three months later you discover `cmd_export` exists in one adapter but not the other.

**Call Parity makes adapter symmetry a CI-checkable property, not a code-review hope.** It's rustqual's most opinionated architecture rule, and one I haven't found a direct equivalent for in any other Rust static analyzer.

## What it checks

Configure adapter layers and a shared target. Minimal example with two adapters:

```toml
[architecture.layers]
order = ["domain", "application", "cli", "mcp"]

[architecture.layers.application]
paths = ["src/application/**"]

[architecture.layers.cli]
paths = ["src/cli/**"]

[architecture.layers.mcp]
paths = ["src/mcp/**"]

[architecture.call_parity]
adapters = ["cli", "mcp"]
target   = "application"
```

`adapters` can list any number of peer layers — REST endpoints, web handlers, gRPC servers, message-queue consumers — they're treated identically.

Two checks run under one rule:

- **Check A — every adapter must delegate.** Each `pub fn` in an adapter layer must (transitively) reach into the `target` layer. A CLI command that doesn't actually call into the application layer is logic in the wrong place. Caught at build time.
- **Check B — every target capability must reach all adapters.** Each `pub fn` in the `target` layer must be (transitively) reached from *every* adapter layer. Add `app::ingest::run`, forget to wire it into CLI, and Check B reports exactly that — by name, in CI, before review.

`call_depth` (default 3) controls how many hops the transitive walk traces.

## Why this is unusual

Static analyzers traditionally fall into two camps:

- **Style and local linters** (Clippy, ESLint, RuboCop) enforce per-function rules. They don't know your architecture.
- **Architecture linters** (ArchUnit, dependency-cruiser) enforce **containment**: "domain doesn't import adapters", "infrastructure doesn't depend on application". They prove what *can't* be called.

Neither proves what **must** be called — that several adapter modules *collectively cover every public capability* of a target module. That requires building a real call graph across files, resolving method receivers through type aliases, wrappers, re-exports, and `Self`, then reverse-walking from each adapter to see what target functions it actually reaches.

I haven't found another tool — for any language — that does this out of the box. The closest neighbours are general-purpose graph queries on top of CodeQL or Joern, where you write the analysis from scratch every time. If you know of one, I'd genuinely like to hear about it.

## The hard part: an honest call graph

The rule itself is simple. The work is making the call graph honest. Real Rust code looks like:

```rust
let session = Session::open_cwd().map_err(map_err)?;
session.diff(path).map_err(map_err)?;
```

A naive analyzer sees `.diff()` on something it can't name and gives up — that turns into a false-positive "your CLI doesn't reach the application." rustqual ships a shallow type-inference engine that resolves receivers end-to-end:

- Method-chain constructors and stdlib combinator returns (`Result::map_err`, `Option::ok`, `Future::await`, `Result::inspect`, …)
- Field access chains (`ctx.session.diff()`)
- Trait dispatch on `dyn Trait` and `impl Trait` (over-approximated to every workspace impl)
- Type aliases — including chains, wrappers (`Box<Hidden>`), and re-exports
- Renamed imports (`use std::sync::Arc as Shared;`) — with shadow detection so a local `crate::wrap::Arc` doesn't masquerade as stdlib
- `Self` substitution across all resolver paths so impl-internal delegation works

Anything that can't be resolved cleanly stays unresolved — no fabricated edges. **False positives kill architectural rules**; missing an edge is recoverable, inventing one isn't.

## Framework extractors and macro transparency

Web frameworks wrap state in extractor types (`State<T>`, `Data<T>`, `Json<T>`). Without help, the call graph stops at the extractor. Add them as transparent wrappers:

```toml
[architecture.call_parity]
adapters = ["cli", "mcp"]
target   = "application"
transparent_wrappers = ["State", "Extension", "Json", "Data"]
transparent_macros   = ["tracing::instrument", "async_trait::async_trait"]
```

Now `fn h(State(db): State<Db>) { db.query() }` resolves through the `State<T>` peel and the `Db::query` edge is recorded.

The default `transparent_macros` list already covers the common cases; entries here extend it.

## What you'll see

```
✗ ARCH-CALL-PARITY  src/cli/commands/sync.rs::cmd_sync (Check A)
                    pub fn does not (transitively, depth=3) reach the target layer
                    'application' — adapter has no delegation path

✗ ARCH-CALL-PARITY  src/application/export.rs::run_export (Check B)
                    target fn is unreached by adapter 'cli'
                    (reachable from: mcp)
```

The first finding says "this CLI command does logic locally instead of delegating". The second says "you added a new application capability and forgot to expose it via CLI".

## Excluding legitimate asymmetries

Sometimes a target function genuinely shouldn't have an adapter for every frontend — debug endpoints, admin-only tooling, internal setup. Use `exclude_targets`:

```toml
[architecture.call_parity]
adapters = ["cli", "mcp"]
target   = "application"
exclude_targets = [
    "application::admin::*",     # admin tools, not exposed via either adapter
    "application::setup::run",   # bootstrap, called once at startup
]
```

Globs match against the *module path* (with `crate::` stripped), not the layer name. `application::admin::*` matches every `pub fn` under `src/application/admin/**`.

For ad-hoc per-function suppression:

```rust
// qual:allow(architecture) — internal capability, intentionally MCP-only
pub fn admin_purge() { /* … */ }
```

## Why the false-positive rate matters

False positives don't just waste reviewer time, they *teach the team to ignore the tool*. The whole call-parity approach only works if the false-positive rate stays close to zero — which is why the receiver-type-inference engine refuses to fabricate edges. An honest "I don't know" beats a confident wrong answer when the rule is going to fail builds.

## For teams using AI coding assistants

If you're building Rust with Copilot, Claude, Codex, or similar: this rule guards against one of the more common patterns of architectural drift in AI-assisted codebases. When an agent adds `pub fn export_csv()` to your application layer, it tends to wire it into one frontend and forget the others. Check B catches that on the next `cargo` run — before the PR — without you having to write a custom prompt or review checklist.

Combined with rustqual's other architecture rules (layers, forbidden edges, trait contracts), this gives any LLM agent a *structural* feedback loop that's stricter and more reliable than narrative architectural documentation in a repo's README.

## Try it

```toml
# rustqual.toml
[architecture]
enabled = true
[architecture.layers]
order = ["domain", "application", "cli", "mcp"]
# ... layer paths ...

[architecture.call_parity]
adapters = ["cli", "mcp"]
target   = "application"
```

```sh
cargo install rustqual
rustqual . --fail-on-warnings
```

## Related

- [architecture-rules.md](./architecture-rules.md) — the broader architecture dimension (layers, forbidden edges, patterns, trait contracts)
- [ai-coding-workflow.md](./ai-coding-workflow.md) — why call parity especially matters for AI-generated code
- [reference-rules.md](./reference-rules.md) — every rule code with details
- [reference-configuration.md](./reference-configuration.md) — every config option
