# Golden Example: `forbidden`

This mini-fixture demonstrates the **Forbidden Rule**
(`[[architecture.forbidden]]`) of the rustqual Architecture dimension.

## What the rule says

Each rule has a `from` and a `to` file-path glob plus an optional list
of `except` globs. A file matching `from` must not import anything that
resolves to a file matching `to`, unless the same resolution also hits
one of the `except` globs.

## What's in this example

Two peer analyzer modules under `src/adapters/analyzers/`. The rule says:

- `from = "src/adapters/analyzers/iosp/**"`
- `to   = "src/adapters/analyzers/**"`
- `except = ["src/adapters/analyzers/iosp/**"]`

so an `iosp` file may freely import from its own tree but not from any
other peer analyzer.

- `src/adapters/analyzers/iosp/bad.rs` — violates the rule by importing
  `crate::adapters::analyzers::srp::measure`.
- `src/adapters/analyzers/iosp/mod.rs` — declares the bad submodule.
- `src/adapters/analyzers/srp/mod.rs` — the forbidden peer.

## Expected finding

The Forbidden Rule produces exactly **one** hit for this fixture:

- **Rule**: `architecture::forbidden`
- **Kind**: `ForbiddenEdge { reason: "peer analyzers are isolated" }`
- **Hit line**: 1 of `src/adapters/analyzers/iosp/bad.rs`

## Rule-level expectations (used by snapshot tests)

`check_forbidden_rules` with the compiled rule above returns one
`MatchLocation` whose `ViolationKind::ForbiddenEdge.reason ==
"peer analyzers are isolated"` and whose `imported_path` starts with
`crate::adapters::analyzers::srp`.
