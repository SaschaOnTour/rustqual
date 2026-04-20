# Golden Example: `forbid_macro_call`

Demonstrates the `forbid_macro_call` matcher of the Architecture dimension.

## What the rule says

Macro invocations (`name!(...)`) whose final path segment matches a
banned name are flagged in the configured scope. Typical use: forbid
`println!`, `eprintln!`, `panic!`, `todo!` in library / production code.

## What's in this example

`src/domain/bad.rs` contains one banned macro call: `println!(...)`.

- **Rule**: `architecture::pattern::no_stdout_in_library_code` (in `rustqual.toml`)
- **Kind**: `forbid_macro_call`
- **Expected hits**: 1

## Matcher-level expectations (used by snapshot tests)

`find_macro_calls` run against `src/domain/bad.rs` with `names = ["println"]`
returns exactly **one** `MatchLocation` with
`ViolationKind::MacroCall { name: "println" }`.
