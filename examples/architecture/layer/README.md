# Golden Example: `layer`

This mini-fixture demonstrates the **Layer Rule** of the rustqual
Architecture dimension.

## What the rule says

Files are ranked by the layer their path glob matches. An inner layer
(lower rank) may not import from an outer layer (higher rank). With
`order = ["domain", "adapter"]`, `domain` has rank 0 and `adapter` has
rank 1, so `src/domain/**` files must not import from `src/adapters/**`.

## What's in this example

- `src/domain/bad.rs` — a `domain` file that imports `crate::adapters::Foo`
  (inner importing outer → violation).
- `src/adapters/mod.rs` — a trivial `adapter` module that defines the
  imported symbol.
- `src/adapters/extra.rs` — a sibling `adapter` file so the layer glob
  matches more than one path (keeps the example realistic).

## Expected finding

The Layer Rule produces exactly **one** hit for this fixture:

- **Rule**: `architecture::layer`
- **Kind**: `LayerViolation { from_layer: "domain", to_layer: "adapter" }`
- **Hit line**: 1 of `src/domain/bad.rs` (the `use` statement)

## Rule-level expectations (used by snapshot tests)

`check_layer_rule` when run against the parsed fixture files with the
`rustqual.toml`-derived `LayerDefinitions` returns one `MatchLocation`
whose `ViolationKind::LayerViolation` has `from_layer == "domain"`
and `to_layer == "adapter"`.
