# `architecture.call_parity` — Golden Example

A toy workspace that demonstrates the two checks `[architecture.call_parity]` runs:

- **no_delegation (Check A)** — every `pub fn` in an adapter layer
  must transitively delegate into the target (application) layer.
- **missing_adapter (Check B)** — every `pub fn` in the target
  layer must be reached from every adapter layer.

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
— both checks piggy-back on the layer definitions already in place for
the architecture dimension.
