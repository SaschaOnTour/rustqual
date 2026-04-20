# Golden Example: `forbid_glob_import`

This mini-fixture demonstrates the `forbid_glob_import` matcher of the
rustqual Architecture dimension.

## What the rule says

`use foo::*;` style glob imports may be forbidden in certain paths because
they can silently tunnel re-exports across layer boundaries. The matcher
flags every glob import regardless of source; the scope is decided by the
rule configuration.

## What's in this example

`src/domain/bad.rs` contains one glob import in a domain-layer file,
which the matcher flags. The expected finding:

- **Rule**: `architecture::pattern::no_glob_imports_in_domain` (configured in `rustqual.toml`)
- **Kind**: `forbid_glob_import`
- **Hit line**: 1 (the `use some_crate::*;` statement)
- **Base path**: `some_crate`

## Matcher-level expectations (used by snapshot tests)

The AST-level matcher `find_glob_imports` when run against
`src/domain/bad.rs` returns exactly **one** `MatchLocation` whose
`ViolationKind::GlobImport.base_path == "some_crate"`.
