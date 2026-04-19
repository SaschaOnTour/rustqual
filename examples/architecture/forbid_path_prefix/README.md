# Golden Example: `forbid_path_prefix`

This mini-fixture demonstrates the `forbid_path_prefix` matcher of the
rustqual Architecture dimension.

## What the rule says

Symbols starting with the configured prefixes (e.g. `tokio::`) may only
appear in files matching specific path globs. Everywhere else they are
violations.

## What's in this example

`src/domain/bad.rs` contains a single-import violation — a domain-layer
file importing from `tokio::`. The expected finding:

- **Rule**: `architecture::pattern::syn_only_in_adapters` (configured in `rustqual.toml`)
- **Kind**: `forbid_path_prefix`
- **Hit line**: 1 (the `use tokio::spawn;` statement)
- **Rendered path**: `tokio::spawn`

## Matcher-level expectations (used by snapshot tests)

The AST-level matcher `find_path_prefix_matches` when run against
`src/domain/bad.rs` with prefix `"tokio::"` returns exactly **one**
`MatchLocation` whose `ViolationKind::PathPrefix.prefix == "tokio::"`.
