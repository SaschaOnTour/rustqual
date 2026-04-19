# Golden Example: `forbid_function_call`

This mini-fixture demonstrates the `forbid_function_call` matcher of the
rustqual Architecture dimension.

## What the rule says

Symbols configured in `forbid_function_call` are matched against the
**full rendered path** of call expressions. `forbid_function_call =
["Box::new"]` matches `Box::new(x)` but not `Thing::new()` and not
`x.new()` (the latter is a method call — `forbid_method_call` territory).

## What's in this example

`src/domain/bad.rs` contains a single-call violation — the domain layer
constructing a `Box<T>` via `Box::new`. The expected finding:

- **Rule**: `architecture::pattern::no_boxed_allocations_in_domain`
- **Kind**: `forbid_function_call`
- **Hit line**: 4 (the `Box::new(42)` expression)
- **Rendered path**: `Box::new`

## Matcher-level expectations (used by snapshot tests)

The AST-level matcher `find_function_call_matches` when run against
`src/domain/bad.rs` with path `"Box::new"` returns exactly **one**
`MatchLocation` whose `ViolationKind::FunctionCall.rendered_path ==
"Box::new"`.
