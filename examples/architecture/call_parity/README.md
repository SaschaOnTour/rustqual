# `architecture.call_parity` — Golden Example

A toy workspace that demonstrates `[architecture.call_parity]`. The
rule actually runs **four checks** A/B/C/D — this fixture exercises
A and B (the two coverage checks); Check C (`single_touchpoint`,
default `"warn"`) emits at `Severity::Low` if a handler reaches more
than one target node, and Check D (`multiplicity_mismatch`) flags
divergent per-adapter handler counts:

- **no_delegation (Check A)** — every `pub fn` in an adapter layer
  must reach the target (application) layer at the boundary
  (forward BFS, stops on first target hit).
- **missing_adapter (Check B)** — every `pub fn` in the target layer
  must be reached from every adapter layer (or be transitively
  reachable from some adapter touchpoint via target-internal callers).
- **multi_touchpoint (Check C)** — `single_touchpoint = "warn"` by
  default, so a handler that orchestrates across more than one
  target capability emits a low-severity finding.
- **multiplicity_mismatch (Check D)** — a target reached by every
  adapter must be reached with the same handler count from each.

## Layout

```
src/
├── application/     target layer — shared business logic
│   ├── stats.rs     pub fn get_stats
│   └── list.rs      pub fn list_items
├── cli/
│   └── handlers.rs  cmd_stats, cmd_list, cmd_debug (qual:allow)
├── mcp/
│   └── handlers.rs  handle_stats, handle_list
└── rest/
    └── handlers.rs  post_stats (delegates), post_list (inlined)
```

## Expected findings

Running `cargo run -- examples/architecture/call_parity --findings`
produces exactly two findings:

1. `architecture/call_parity/no_delegation` on `post_list` in
   `src/rest/handlers.rs` — it returns a hard-coded Vec instead of
   calling `application::list::list_items`.
2. `architecture/call_parity/missing_adapter` on
   `crate::application::list::list_items` — CLI + MCP both delegate
   into it, but REST doesn't, so the coverage set `{cli, mcp}` is
   missing `rest`.

`cmd_debug` carries `// qual:allow(architecture)` so the pipeline
silences its would-be `no_delegation` finding. This is the explicit
escape for intentionally asymmetric features.

## Wiring

`rustqual.toml` configures `[architecture.layers]` plus a single
`[architecture.call_parity]` section. No per-function annotation needed
— all four checks (A/B/C/D) piggy-back on the layer definitions
already in place for the architecture dimension. This fixture
exercises the A and B findings; C and D fire on different shapes
that this minimal example doesn't include.
