# Golden Example: `forbid_item_kind`

This mini-fixture demonstrates the `forbid_item_kind` matcher of the
rustqual Architecture dimension.

## What the rule says

`forbid_item_kind = ["unsafe_fn", …]` bans specific language-level item
shapes from appearing in files matched by the pattern's scope. Supported
kinds (Phase 6):

| Kind | Matches |
| --- | --- |
| `async_fn` | any `async fn` |
| `unsafe_fn` | any `unsafe fn` |
| `unsafe_impl` | `unsafe impl Trait for Type` |
| `static_mut` | `static mut NAME: T` |
| `extern_c_block` | `extern "…" { … }` |
| `inline_cfg_test_module` | `#[cfg(test)] mod name { … }` with body |
| `top_level_cfg_test_item` | `#[cfg(test)] fn/impl/const` at top level |

## What's in this example

`src/domain/bad.rs` contains a single `unsafe fn` — the simplest kind
hit. The expected finding:

- **Rule**: `architecture::pattern::no_unsafe_fn_in_domain`
- **Kind**: `forbid_item_kind` (`unsafe_fn`)
- **Hit line**: 1 (the `pub unsafe fn …`)
- **Item name**: `dangerous`

## Matcher-level expectations (used by snapshot tests)

The AST-level matcher `find_item_kind_matches` when run against
`src/domain/bad.rs` with kind `"unsafe_fn"` returns exactly **one**
`MatchLocation` whose `ViolationKind::ItemKind.kind == "unsafe_fn"`
and `name == "dangerous"`.
