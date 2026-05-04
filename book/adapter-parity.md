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

Four checks run under one rule, all anchored at the **boundary** — the first call from an adapter into the target layer:

- **Check A — every adapter must delegate.** Each `pub fn` in an adapter layer must reach into the `target` layer. A CLI command that doesn't actually call into the application layer is logic in the wrong place. Caught at build time.
- **Check B — touchpoint sets must match.** Each target `pub fn` reached from one adapter must be reached from every adapter (or excluded explicitly). Add `app::ingest::run`, forget to wire it into CLI, and Check B reports exactly that — by name, in CI, before review.
- **Check C — single touchpoint per handler.** Each adapter `pub fn` should have exactly one touchpoint in the target layer. Multi-touchpoint handlers orchestrate across application calls themselves — that orchestration logic risks divergence between adapters. Configurable severity (`single_touchpoint = "off" | "warn" | "error"`, default `warn`).
- **Check D — multiplicity must match.** When two adapters both reach the same target capability, they must reach it with the same handler count. cli having `cmd_search` + `cmd_grep` (alias) both reach `session.search` while mcp has only `handle_search` is API surface drift, even though Check B is silent.

### Touchpoints — what counts and what doesn't

A **touchpoint** is the first node in the target layer reached when walking forward from an adapter pub-fn through adapter-internal helpers. The walk stops on first target hit and does not descend into target callees. Two kinds of nodes count as target boundaries:

- **Concrete target pub-fns** — direct calls into application-layer functions (`session.search`).
- **Trait-method anchors** — synthetic `<Trait>::<method>` nodes emitted by `dyn Trait.method()` dispatch, recognised as a boundary when (a) the trait declaration is NOT in a peer-adapter layer, AND (b) the trait declaration IS in the target layer with a callable body (default OR overriding impl), OR at least one overriding impl lives in the target layer. The anchor itself can live in any non-peer-adapter layer (typically `ports` for Hexagonal, or `application` itself for in-target traits). Check B/D enumerate anchors alongside concrete target pub-fns when computing coverage and multiplicity, and skip concrete impl-method pub-fns that an enumerated anchor already covers — **but only when no adapter has the concrete in its boundary coverage**. When an adapter reaches the capability via direct concrete call (`LoggingHandler::handle()` UFCS or static-method form) and another via `dyn Trait` dispatch, the concrete pass still runs so mixed-form drift surfaces as paired concrete + anchor findings (cross-form synonym handling stays intentionally out of scope). Anchor findings carry the trait method's actual declaration line so suppression markers and SARIF locations work correctly.

The walker refuses to promote anchors declared in **peer-adapter layers** (e.g. `mcp::Handler` from a `cli`-origin walk) to target boundaries — otherwise `cli` could inherit `mcp`'s coverage by dispatching through MCP's port trait.

This boundary stop is deliberate: application-internal call chains (`session.search → record_operation → impact_count`) aren't a parity concern. If two adapters both reach `session.search`, the parity question is answered. What `session.search` does internally is `DRY-002`'s job, not `call_parity`'s.

`call_depth` (default 3) bounds the **adapter-internal** traversal — how many helper hops the walk will go through before giving up. It does not constrain post-boundary application chain depth.

### Deprecated-handler exclusion

Adapter `pub fn`s marked with `#[deprecated]` (in any form: bare,
`#[deprecated = "..."]`, or `#[deprecated(since = "...", note = "...")]`)
are excluded from Checks A/B/C/D. Aliases that are explicitly being
phased out shouldn't drag the parity report.

```rust
#[deprecated = "use cmd_search"]
pub fn cmd_grep(args: ClapArgs) { /* … */ }   // skipped from parity
```

### Limitations: type aliases

The visibility pass and the receiver-type inference agree on most
alias shapes (`pub type Public = Box<private::Hidden>`, multi-step
chains, transparent-wrapper peeling), but two alias patterns currently
disagree and can produce false-negative Check-B coverage:

1. **Generic-identity aliases at use sites.** A `type Id<T> = T;` and
   then `pub type Public = Id<private::Hidden>;` registers `Id` in
   the visible-types set, but receiver inference resolves callers
   `x: Public` directly to `Hidden`. Methods on `Hidden` therefore
   stay outside the public surface even though they are reachable
   through `Public`. Workaround: declare the inner type publicly, or
   alias without the generic identity wrapper. A correct fix would
   substitute alias generic parameters at use sites while still
   refusing to peel nominal generic types like `struct Wrapper<T>`.

2. **Impl blocks keyed under an alias.** `pub type Public =
   private::Hidden; impl Public { pub fn op(&self) {} }` keys the
   impl method under `Public::op`, but a caller with `x: Public`
   resolves to `Hidden::op` after alias expansion. Workaround: write
   the impl block against the underlying type (`impl Hidden`). A
   correct fix would chase the alias chain in `resolve_impl_self_type`
   and register methods under every alias-equivalent canonical.

3. **Nested re-exports in receiver position.** `pub mod outer { …
   pub use self::private::Hidden; }` followed by `fn h(x: outer::Hidden)
   { x.op() }` — the receiver-type resolver does not chase workspace
   `pub use` re-exports inside nested modules, so the parameter
   resolves to `crate::…::outer::Hidden` while the impl method lives
   under `crate::…::outer::private::Hidden`. Visibility recognises
   both paths, but the call-graph edge ends up under the short
   `<method>:op` form. Workaround: write the impl at the file-level
   qualified path (`impl outer::Hidden { … }`) so impl-canonical
   and caller-canonical agree, or `qual:allow(architecture)` at the
   call-site.

4. **Public function re-exports.** `mod private { pub fn op() {} }
   pub use private::op;` — the visibility pass chases type
   re-exports through `chase_alias_chain` in `walk_use_tree`, but
   value re-exports (functions, consts, statics) are intentionally
   filtered out (they would otherwise leak same-named private types
   into the visible-type set). The trade-off: a function exposed
   only via `pub use` is missed by the target pub-fn set, so Check
   B/D may not require adapter coverage for it. Workaround: declare
   the function at a publicly-reachable path directly, or wrap it
   in a public struct method.

5. **Calls inside trait default-method bodies.** `trait Handler {
   fn handle(&self) { self.helper(); } }` — `dyn Handler.handle()`
   resolves to the trait-method anchor `<Handler>::handle`, but the
   workspace call graph doesn't model `TraitItemFn::default` as a
   node, so calls **inside** the default body (`self.helper()`)
   stay invisible to Check A/B/D. Workaround: override the method
   explicitly in each impl, or move the body into a free function
   the trait method delegates to.

If you hit any of these patterns in practice, please open an issue
with the exact alias / re-export / dispatch shape — the fixes
touch several call sites in the call-parity pipeline and we'd like
a real-world example to drive the substitution semantics rather
than guessing.

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
- Trait dispatch on `dyn Trait` and `impl Trait` collapses to a single synthetic anchor `<Trait>::<method>` representing the logical capability — the call graph **does not** fan out to per-impl edges, which would otherwise turn one boundary call into N touchpoints and falsely fire Check C. The walker recognises the anchor as a target boundary when at least one overriding impl lives in the target layer.
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

In `--findings` (one-line) output, real findings look like:

```
src/cli/commands/sync.rs:12  ARCHITECTURE  adapter cli::cmd_sync does not delegate to 'application' within 3 hops: call parity
src/application/export.rs:8  ARCHITECTURE  'crate::application::export::run_export' is not reached from adapter layer(s): cli: call parity
```

(In `--format json` these surface under the top-level `architecture_findings[]` array; in `--format sarif` as `result` entries with `ruleId = "architecture/call_parity/no_delegation"` / `"…/missing_adapter"`. Full rule-ID list: [reference-rules.md](./reference-rules.md).)

The first says "this CLI command does logic locally instead of delegating". The second says "you added a new application capability and forgot to expose it via CLI".

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
