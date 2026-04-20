# Golden Example: `forbid_method_call`

Demonstrates the `forbid_method_call` matcher of the Architecture dimension.

## What the rule says

Calls to banned method names — either via direct dot-notation
(`x.unwrap()`) or UFCS form (`Option::unwrap(x)`) — are flagged in the
configured scope. Typical use: forbid `.unwrap()` / `.expect()` in
production code so errors are propagated with typed results.

## What's in this example

`src/domain/bad.rs` contains both forms of the violation — a direct
`.unwrap()` call and a UFCS `Option::unwrap(...)` call. The matcher
reports both.

- **Rule**: `architecture::pattern::no_panic_helpers_in_production` (in `rustqual.toml`)
- **Kind**: `forbid_method_call`
- **Expected hits**: 2 (one `direct`, one `ufcs`)

## Matcher-level expectations (used by snapshot tests)

`find_method_call_matches` run against `src/domain/bad.rs` with
`names = ["unwrap"]` returns exactly **two** `MatchLocation`s:
- one with `ViolationKind::MethodCall { name: "unwrap", syntax: "direct" }`
- one with `ViolationKind::MethodCall { name: "unwrap", syntax: "ufcs" }`
