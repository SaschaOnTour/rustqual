# Golden Example: `forbid_derive`

This mini-fixture demonstrates the `forbid_derive` matcher of the
rustqual Architecture dimension.

## What the rule says

`forbid_derive = ["Serialize"]` bans `#[derive(Serialize)]` (and
`#[derive(serde::Serialize)]`, matched by final segment) on any
struct / enum / union in scope.

## What's in this example

`src/domain/bad.rs` derives `Serialize` on a Domain type — a common
architectural anti-pattern that lets serialization shape bleed into the
Domain layer. The expected finding:

- **Rule**: `architecture::pattern::no_serialize_in_domain`
- **Kind**: `forbid_derive` (`Serialize`)
- **Hit line**: 4 (the `Serialize` token in the derive list)
- **Item**: `Foo`

## Matcher-level expectations (used by snapshot tests)

The AST-level matcher `find_derive_matches` when run against
`src/domain/bad.rs` with name `"Serialize"` returns exactly **one**
`MatchLocation` whose `ViolationKind::Derive.trait_name == "Serialize"`
and `item_name == "Foo"`.
