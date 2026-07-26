# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.8.0] - 2026-07-26

Dead-code detection covered functions only. An unused `struct`, `enum`,
`union`, type alias, `const` or `static` was invisible — and a `// qual:api` on
one of them was reported as inert, because nothing could ever act on it. That
was the loose end left by 1.7.0: the marker had become verified, but on a type
there was nothing to verify it against.

### Added
- **DRY-006: dead types and constants.** A declaration nothing refers to is
  reported as `unused type`; one only test code names, as `type test-only`. The
  model mirrors DRY-002 — declarations on one side, a workspace-wide reference
  set on the other — and the exemptions are the same, so there is no second
  rule to learn: `#[allow(dead_code)]`, `// qual:api`, `// qual:test_helper`.
  Configurable via `[duplicates] detect_dead_types` (default true).

  A reference is any name occurrence: type positions, expression paths,
  patterns, generic bounds, derive arguments and macro token streams. That is
  the grain the call graph already works at, and it is deliberately generous —
  over-collecting only suppresses a finding, while under-collecting invents
  one, and telling an author to delete a type that is in use is the expensive
  mistake.

  Exactly two things are not a reference: a declaration's own name, and the
  self type of an `impl` block. Without the second, a type carrying only its
  own methods would keep itself alive and nothing could ever be found — the
  same verdict rustc reaches with "never constructed". rustc's `dead_code` lint
  stops at the crate boundary, so a `pub` type nobody in the workspace uses
  stays invisible there; that is where this earns its keep.

  **Traits are deliberately out of scope.** Every `impl Trait for X` names the
  trait, so a trait with a single implementation would always look used: the
  check would carry the full false-finding risk without the value.
- **`qual:api` and `qual:test_helper` are verified on types too.** Functions and
  types are now asked the same three questions — is it exempt anyway, can it be
  named from outside the crate, does production use it — with the message
  naming the evidence that applies ("production calls" vs "production refers
  to"). External reachability therefore records public types, constants and
  traits, not just functions; without that every legitimately public type would
  have been accused of not being nameable from outside.
- **Intra-doc links count as references.** `/// see [`MAX`]` names an item that
  no lexer turns into an identifier, and deleting the target breaks the
  documentation — so a bracketed link target is a use. Only bracketed spans
  count; harvesting every word of prose would let any type whose name appears
  in a sentence keep itself alive. Format-string placeholders and doc links are
  the two ways a name hides inside text, and both now go through
  `shared/text_names.rs`.
- **Doc examples count as test uses.** A type named only inside a ``` fence in
  a doc comment is in use — `cargo test` compiles and runs that code — but it is
  *test* use, so it is reported as `type test-only` rather than silently
  ignored. Doc comments reach `syn` as one `#[doc = "…"]` attribute per line, so
  the fence is tracked across them. An intra-doc link stays a production
  reference: it documents the API rather than exercising it, and prose outside a
  fence still contributes nothing but its link targets.
- **JSON gains a `dead_types` array.** Kept apart from `dead_code` rather than
  folded into it: a consumer reading `function_name` must not be handed a
  struct name. Human-facing output lists both in one table, told apart by the
  kind tag.

### Changed
- **The JSON summary and the baseline carry `dead_type_warnings`.** The array
  was there but the count was not, so the two halves of the envelope disagreed;
  the baseline field is `#[serde(default)]`, so older baseline files still load.
- **"Not attached" now names what it means.** A marker that reaches no
  declaration is still reported, but the remedy text names what it could be
  sitting on — a `pub use` re-export, a module, a trait — instead of "a type, a
  constant or a re-export", which stopped being true.
- **Two exemptions came out of running DRY-006 on rustqual itself.** Inline
  format arguments (`format!("{PREFIX}…")`) are one literal to `proc_macro2`,
  so a token walk saw no reference at all; `all_idents` now reads `{…}`
  placeholders, including named spec arguments (`{v:>width$}`). And a leading
  underscore is the language's own "deliberately unused" convention, which
  rustc's `dead_code` lint honours — arguing with the compiler there would only
  be noise.

### Fixed
- **The `dead_code` lint level is modelled as Rust defines it.** Both dead-code
  checks read only a declaration's own attributes, so `#![allow(dead_code)]` at
  the top of a file, `#[allow(dead_code)]` on a module or an `impl` — the
  generated-code idiom, one attribute rather than one per item — and one on an
  enclosing function excused nothing. The level is now tracked down every
  lexical scope for DRY-002 and DRY-006 alike, and it is a *level*, not a
  one-way flag: attributes are folded in source order so a later one overrides
  an earlier one (`#[deny] #[allow]` really is allowed), an inner
  `#[deny(dead_code)]` revokes an inherited `allow`, and `forbid` is the one
  level nothing narrower may relax — for rustc an inner `allow` under it is an
  error, so honouring one would silence what the compiler never would.
  (`visit_all_files` dispatches through the trait for this, so a visitor
  overriding `visit_file` sees a file's inner attributes.)
- **The test-context switch happens on every item.** It fired for modules,
  `impl` blocks and functions but not for the rest, so a reference from a
  `#[cfg(test)]` const, struct or `use` landed in the production set and a
  declaration used only from there produced no finding at all. Both the call
  graph and the reference set now scope at the item dispatch, where every kind
  passes through — along with `#[test]`-family attributes, which the call graph
  previously recognised only on free functions. Attributes are not an item-level
  thing, though, so the same scoping happens at every dispatch that can carry
  one: associated items in an `impl` or a trait, items in an `extern` block,
  struct fields, enum variants, `let` and macro statements, and match arms. A
  `#[cfg(test)]` statement is absent from a non-test build, so what it names is
  a test reference — the enclosing function being production says nothing about
  it. The list lives in one place (`dry/split_names.rs`), because the gap this
  closes is always the same shape — a node kind nobody thought of — and one list
  fixes both the call graph and the reference set at once. The list is derived
  rather than guessed: `syn` has sixty-nine structs with an `attrs` field, and
  they reach a visitor through those dispatches or as their own node, so every
  attributed position an author can write is covered — down to an array element,
  a call argument, a struct field value and a pattern field. Reaching them needs
  exhaustive matches over `syn::Expr` and `syn::Pat` (there is no shared
  accessor) plus a step to the first operand of an assignment, where `syn` binds
  what rustc reads as the statement's attribute. The signature and generic
  positions are covered too — `fn(#[cfg(test)] Fixture)` and
  `struct Holder<#[cfg(test)] T = Fixture>` both compile and both drop the only
  mention of the type outside a test build. `File` is the one attributed node
  handled elsewhere: its inner attributes are what the whole-file test
  classification reads.
- **SLM missed `self` inside an inline format argument.** `format!("{self:?}")`
  is one literal to `proc_macro2`, so the token scan saw no `self` and reported
  the method as self-less. Same root cause as the DRY-006 case above, same fix:
  `tokens_reference_ident` reads the placeholders. It can only turn findings
  off, matching the check's documented bias.
- **NMS missed mutation through a nested field.** `self.inner.items.push(v)`
  mutates `self` just as `self.items.push(v)` does, but the self-target test
  matched exactly one field level, so the method was reported as taking a
  needless `&mut self` — advice that would not even compile. The chain is now
  walked to its root through field, index and paren expressions, which also
  subsumes the separate `self.field[i]` case. Strictly more conservative: it
  can only turn findings off.

## [1.7.0] - 2026-07-26

Release closing the last blind spot in the suppression system: `// qual:api`
and `// qual:test_helper` were **permanent, unverified silencers**. Unlike
`qual:allow` — which re-fires and is reported as `ORPHAN_SUPPRESSION` when it
stops covering anything — the two bare markers kept working forever once
written. That made a marker ambiguous: it could mean "real API entry point" or
"dead code nobody noticed", and the two are indistinguishable without checking
every case by hand. Genuine rot hides behind the second reading indefinitely.

Dogfooding found **59 spent markers in rustqual's own code** on the first run;
all are removed in this release.

### Added
- **Stale `qual:api` / `qual:test_helper` detection.** Both markers exist to
  excuse a function *production never calls*, so once production calls it the
  excuse is spent and the marker is reported as `ORPHAN_SUPPRESSION`. The rule
  deliberately does **not** also require the function to be tested: TQ-003
  (untested) only fires for functions that already have production callers, so
  for a genuine outside-the-crate entry point that exclusion never applied
  anyway — requiring "and tested" would only let a spent marker keep hiding a
  real TQ-003 finding. Removing a spent marker can therefore surface an
  `untested` finding; the message says so up front.
- **`qual:api` on a crate-internal item is reported as a category error.** A
  marker on something no outside consumer can name — behind a private `mod`,
  not `pub`, or anywhere in a binary — never applied in the first place. The
  message says why (*"cannot be called from outside the crate"*) and what to
  do: remove the marker, or call the function from production / delete it.
  External reachability (`adapters/shared/reachability/`) is derived from the
  `.rs` set alone: the module tree is **walked** from every crate root, so a
  module's logical path and the file implementing it agree by construction —
  then the `mod` visibility chain decides, plus `pub use` name and glob
  re-exports. Every uncertainty resolves to *reachable*, so an unrecognised
  layout can never manufacture a false finding.
- **Markers that reach no function are reported.** Both markers only affect
  the function-level checks (DRY-002, TQ-003), so one sitting on a type, a
  constant or a `pub use` re-export provably does nothing — as does one on a
  function both checks already exempt (`main`, a test fn, a trait-impl method,
  `#[allow(dead_code)]`). Attachment mirrors the marking pass exactly (same
  annotation window), so a working marker can never be called unattached.
- **`marker` field on JSON orphan entries** (`"allow"` / `"api"` /
  `"test_helper"`). A bare `qual:api` carries no dimensions and no target, so
  without it a consumer could not tell it from a blanket `qual:allow` — and
  would report the wrong remedy.

### Changed
- **Orphan markers render by their real name in every reporter.** The
  `qual:allow(...)` string was duplicated across the text, SARIF, GitHub, AI
  and HTML reporters; all now go through one `OrphanSuppression::marker_spec`,
  so a stale `qual:api` no longer prints as a meaningless
  `qual:allow(<all>)`. The HTML orphan table's *Scope* column is now *Marker*.
- **`qual:test_helper` is judged by callers only** — being unreachable from
  outside the crate is its normal, intended state. The "helper nobody calls"
  case stays with DRY-002, which deliberately does not suppress its `uncalled`
  variant for this marker; reporting it here too would double-report one
  defect.
- **59 spent `qual:api` markers removed from rustqual's own source.** All sat
  on crate-internal items (`mod adapters;` and friends are private, so the
  `pub` keyword is inert there) that production already calls.

### Fixed
- **Ambiguous names never mark a marker spent.** The call collector records a
  path call by its last segment, so a production call to `module_b::handle`
  puts a bare `handle` into the call set. When another declared function shares
  that bare name the call cannot be attributed, and claiming it would tell the
  author to delete a marker that is still holding back a finding — so the
  detector stays silent. A qualified `Type::method` match is specific enough
  and still counts.
- **Workspace crates no longer collide.** A module's identity is its crate
  root plus its logical path, so `crates/a` and `crates/b` can both have an
  `api` module without one marking the other's file reachable — down to the
  visit key of the walk itself, so a file pulled into two crates by `#[path]`
  is judged separately for each and a private `mod` in one crate cannot hide
  what the other publishes.
- **Module files are located by rustc's own rules, in one shared place.**
  `adapters/shared/child_paths.rs` owns them for both consumers (cfg-test
  classification and reachability), so the two cannot drift: a module's
  children live in its *module directory* — the declaring file's directory for
  `mod.rs` / `lib.rs` / `main.rs`, otherwise a directory named after the file's
  stem, extended by the surrounding inline `mod {}` blocks. `#[path]` overrides
  the name but not the base, except at a file's top level, where rustc resolves
  it against the file's own directory. `.` and `..` segments are resolved, since
  `src/a/../shared/api.rs` never equals the recorded `src/shared/api.rs` as a
  string. A `mod` that fails to resolve leaves its file unwalked — and an
  unwalked file counts as reachable, so the mistake was invisible in the output
  but would surface as a false "marker never applied" as soon as a second,
  private module tree claimed the same file.
- **cfg-test classification sees `mod` declarations inside inline blocks.** It
  scanned only a file's top-level items, so a `#[cfg(test)] mod tests;` nested
  in `mod helpers { … }` was never found and the test-only file it names was
  analysed as production — the wrong direction, since that surfaces findings in
  test code. Declarations are now collected with their inline chain (which is
  also where their files live: `helpers/tests.rs`, not `tests.rs`), and a
  `#[cfg(test)]` on an inline block covers everything it declares.
- **Cargo's autobinary forms decide what starts a crate tree.** Both consumers
  now share `adapters/shared/crate_roots.rs`: `src/lib.rs`, `src/main.rs`,
  `src/bin/<name>.rs` and `src/bin/<name>/main.rs`. A deeper file such as
  `src/bin/tools/helper.rs` is a *module* of some binary, not a root — treating
  it as one made it "known but externally unreachable" instead of leaving it
  unknown, which is the difference between reporting a `qual:api` on it and
  leaving it alone. The directory form is newly recognised as a package root
  for integration-test classification too.
- **A `pub use` re-export excuses only the item it names.** Re-exports were
  recorded by bare name, so `pub use public_impl::run` made *every* `run` in
  the workspace look externally reachable. They resolve to the source module's
  file now: `super::` prefixes, unprefixed paths (resolved both crate-root and
  module-relative, since uniform paths allow either), inline modules that have
  no file of their own, renames that change the name mid-chain, multi-step
  façade chains and glob preludes — and only when the re-exporting file is
  itself reachable, so a `pub use` in a private module exposes nothing.
- **A name collision no longer hides a marker that never applied.** External
  reachability is decidable on its own, so the ambiguity brake now only blurs
  *spent* vs *uncalled*; the message says which part is uncertain.
- **Markers inside string literals are no longer collected as annotations.**
  Test fixtures embed rustqual's own markers as data
  (`let code = r#"… // qual:api …"#;`); the raw line scan read those as real
  annotations on the enclosing file. Harmless while markers were never
  verified — but it would have produced phantom findings now, so marker
  collection is column-aware: a marker only counts when it starts outside
  every string-literal span, including literals nested in macro token groups
  (`fixture!({ r#"…"# })`), which neither `syn` nor a top-level token scan
  reaches.
  A line-based filter could not do this — the closing line of a fixture holds
  both literal text and real source, so `// qual:api example"#;` would still
  have registered.

### Notes
- The check rides along with the test-quality pass, because that is where the
  marked declarations and the production call set already exist. With
  `[test_quality] enabled = false` it does not run.
- Item identity in the reachability derivation is `(file, name)` (issue #40),
  so two
  same-named functions in different inline modules of one file cannot be told
  apart — the public one makes the private one look reachable and a stale
  marker there goes unreported. That is the safe direction (a missed finding,
  never a wrongly-demanded deletion); a qualified item key would have to be
  threaded through `DeclaredFunction` and the analyzers sharing it, and its
  payoff is capped because call sites are recorded by last path segment.
- Dead-code detection covers **functions only**, so an unused `struct`,
  `enum`, type alias or `const` is still not reported. That is why a
  `qual:api` on such an item can only ever be inert — and why it is now
  reported instead of silently doing nothing.
- A caller invisible to the call graph (dynamic dispatch, macros) reads as "no
  production callers", so the marker is left alone — the safe direction:
  under-report, never a false "delete me".

## [1.6.1] - 2026-07-25

Bugfix release.

### Fixed
- **SRP-001 (struct cohesion) no longer pools methods across same-named
  structs in different files / modules.** The struct collector keyed method
  buckets by the *bare* last path segment of the type name, so two unrelated
  `struct RunCtx` definitions in different crates shared one bucket: each was
  scored against the other's methods, producing garbage LCOM4 / method-count
  attributed to an innocent, cohesive struct. Both collectors now qualify the
  pooling identity with an `owner_key` (`file::inline_mod_path::name`), so
  methods pool only within their actual owner — mirroring the module
  qualification the DRY `repeated_match` collector already does. (Reported as
  RQ-1 from a downstream workspace.) A *relative* qualified impl self-type
  (`impl inner::Foo`, `impl super::Foo`, `impl self::Foo`) is resolved against
  the impl's inline-module stack so it still keys to — and pools with — its
  struct. Absolute paths (`crate::…`, `::ext::…`) and genuinely cross-file
  split impls are not resolved (the `file + inline-stack` key does not model
  the crate module hierarchy without the fragile file-path→module mapping
  rustqual avoids); they simply do not pool — a safe-direction under-report,
  never a false positive. The SRP analyzer's AST collectors moved into
  `srp/collect.rs` to keep `srp/mod.rs` under the module-length cap. (Landed on
  `main` after the v1.6.0 tag; originally mis-filed under 1.6.0.)
- **`ORPHAN_SUPPRESSION` findings were rendered twice in the text report**
  (issue #36). The text reporter fed the compact findings list from
  `collect_all_findings(...)` — which already includes orphan entries — and
  then appended the snapshot orphans on top, so every `ORPHAN_SUPPRESSION`
  appeared twice (e.g. 3 stale markers → 6 lines). The compact list and the
  summary now use the collected list as-is; the snapshot orphans feed only the
  verbose dedicated section. JSON/SARIF/AI were already correct (single
  entries). The related concern in the same issue — a `qual:allow(dry,
  duplicate)` marker that clears a duplicate group being flagged orphan — was
  already resolved by the v1.5.0 targeted-suppression rework (the orphan
  detector reads pre-suppression positions, so a marker covering a now-silenced
  duplicate still matches its own kind); a regression test now pins it.
- **`cargo doc` warning** in the `[architecture]` config module: TOML section
  references like `[architecture.layers.<name>]` were read as an intra-doc link
  plus an unclosed `<name>` HTML tag. Wrapped them in code spans.

## [1.6.0] - 2026-06-13

Release making rustqual **self-explaining for human and agent consumers**
(rule cards, `--explain <RULE-ID>`, a never-dead-end CLI) and making **BP-002
honest**: trivial-`Display` detection is now semantic (what the body means)
instead of syntactic (which macro it uses), with a config-declared house-idiom
policy. Driven by two field reports: an agent flailing through
`--explain BP-009` → `os error 2` with no next step, and a BP-002 finding
"fixed" by mechanically rewriting one `write!` into two `write_char` calls —
syntax-based matching invited evasion instead of a decision.

### Added
- **Rule cards — one registry, every reporter** (`src/domain/rule_cards/`).
  One card per catalog rule (id, title, what it detects, why it matters, fix
  forms, the copyable suppression marker, the governing config knob). The
  SARIF `tool.driver.rules` table renders from it (a sync test pins set
  equality), `--explain` renders from it, and the compact findings view takes
  its titles from it.
- **`--explain <RULE-ID>`** prints the rule card, case-insensitively
  (`rustqual --explain bp-009` works). Dynamic hierarchical ids resolve to
  their longest registered prefix card — `architecture/pattern/<name>` and
  `architecture/trait_contract/<check>` are exactly what findings print, so
  they explain as their family card. The natural first guess of every
  consumer now succeeds.
- **`--explain` never dead-ends.** An argument that is no rule id, not
  `allow`, and not a readable file prints the three explain modes with
  copyable examples instead of stopping at the bare OS error; an argument
  matching a `qual:allow` *target* name (`--explain boilerplate`) gets a hint
  naming its dimension and the allow guide. The clap help text names all
  three modes.
- **`[boilerplate].accepted_display_idioms`** (fixed vocabulary: `write_str`,
  `write_char`, `write_macro`, `delegation`; default empty) declares the
  project's trivial-`Display` house idiom as policy. Validation is fail-loud
  at startup: an unknown entry is a config error naming the offender and the
  valid vocabulary — never a silent no-op.

### Changed
- **BP-002 matches semantically.** Any branch-free `fmt` body consisting only
  of formatter write ops — `write!(f, …)`/`writeln!(f, …)` (the first macro
  argument must be the formatter; writing to any other target is real
  logic), `f.write_str`, `f.write_char`, `Display::fmt` delegation — is
  trivial, multi-statement bodies included.
  The evasion rewrite (two `write_char` statements) is the same finding; the
  semantically identical `f.write_str(&self.0)` newtype form no longer slips
  through unmatched. With accepted idioms declared, the rule enforces
  house-idiom consistency (a stray `write!` still fires) instead of going
  silent. *Projects with hand-rolled write-only Displays get new BP-002
  findings until they pick a form — declare the idiom, derive, or generate
  via a local `macro_rules!`.*
- **BP-002's suggestion names all fix forms** regardless of workspace
  dependencies (availability ≠ usability under a dependency policy): derive
  (`derive_more::Display` when `suggest_crates`), the accepted-idiom config,
  and a local `macro_rules!` as the dependency-free DRY option; it ends with
  `rustqual --explain BP-002`.
- **The findings epilogue is tail-proof.** It now names both next steps —
  `rustqual --explain <RULE-ID>` (rule details) and `rustqual --explain allow`
  (suppression syntax) — and renders last, so it survives an agent's
  `| tail` truncation. Boilerplate entries in the compact findings view carry
  the registry title (`BP-009 Struct update boilerplate`) instead of the bare
  pattern id.

## [1.5.1] - 2026-06-09

Release fixing a class of **macro-token blindness** that ran across the
codebase, plus a new (backward-compatible) `allow_prelude_glob` architecture
option. syn's visitor
treats a macro body as an opaque token stream, so calls, `self` uses, and
component renders inside `vec![…]`, `format!(…)`, and DSL macros like dioxus
`rsx!{…}` were invisible to every analyzer that only walks the AST — producing
false DEAD_CODE, TQ_NO_SUT, TQ_UNTESTED, and SRP-cohesion findings, and *missed*
forbidden calls in the architecture matchers. Seven sites had each reinvented a
weak `Punctuated<Expr, Comma>` parse blind to the `;`-repeat and block-bodied
forms; they now share one recovery helper.

### Added
- **`adapters/shared/macro_tokens.rs`** — the single source of macro-body
  recovery. `recover_exprs` escalates comma-list → braced-block (handles
  `vec![x; n]` repeat and `;`-separated bodies, lifting trailing `Stmt::Macro`
  back to an `Expr::Macro` and keeping a `let … else { … }` diverging branch) →
  single-expr. `idents_in_call_position` /
  `tokens_reference_ident` are the raw, never-fail fallback for DSL bodies
  (`rsx!{ Component { .. } }`) that parse as no structured expression.
- **`[[architecture.pattern]] allow_prelude_glob` (bool, default `true`).** When
  a pattern sets `forbid_glob_import = true`, idiomatic `*::prelude::*` re-export
  globs (`std`/`dioxus`/`bevy`; a `prelude` segment at any depth) are exempt by
  default. Set `allow_prelude_glob = false` to forbid even prelude globs. The
  `forbid_glob_import` matcher stays a "find all globs" primitive — the prelude
  exemption is a policy decision applied at the analyzer layer, so projects keep
  full control (unlike a hard-coded matcher exemption).

### Fixed
- **Macro-embedded calls are now seen across all call-graph / cohesion sites.**
  Dead-code (DRY-002), TQ-SUT (TQ-002), TQ reachability (TQ-003), SRP cohesion
  (LCOM4), and the architecture `forbid_function_call`/`forbid_method_call`/
  `forbid_macro_call` matchers all route macro bodies through `recover_exprs`,
  so a call inside `vec![helper(); 3]` or a method inside `format!(…)` is no
  longer dropped. The reachability consumers additionally harvest idents in
  *call/construction position* — a call `f(..)` (any case) or an UpperCamelCase
  component/struct render `Component { .. }` (lowercase DSL element tags like
  `div { .. }` and prop keys are excluded, so an unused `fn div()` is never
  masked) — so a dioxus component referenced only via `rsx!{ Component { .. } }`
  (struct syntax the structured parse sees but does not record as a call) is
  recognised as used — clearing false DEAD_CODE / NO_SUT on component-heavy UI
  crates. This is the safe direction for findings: an extra recovered reference
  only ever *suppresses* a finding, never raises a false one (a rare colliding
  name can mask a true positive — the accepted conservative bias). The
  architecture `forbid_*` matchers deliberately use only the structured path,
  never the positional fallback, since over-collection there would manufacture
  false violations.
- **SLM (self-less method) no longer false-fires on macro `self` use.** A `self`
  referenced only inside `format!("{}", self.x)`, `write!(…)`, or any macro is
  now found via a raw token scan (`tokens_reference_ident`), replacing the old
  `matches!`-only special case.

### Notes
- **IOSP own-call counting stays deliberately macro-blind.** Counting own-calls
  inside macro bodies would newly flag real macro-driven render code (a
  component holding a conditional while rendering own components in `rsx!`) as
  an IOSP Violation — a breaking change out of scope here. This is the safe
  direction (under-reports, never false-positive) and is pinned by a
  characterization test.

## [1.5.0] - 2026-06-05

Minor release with one **breaking** config change. rustqual stops shipping a
blunt name-based ignore for `main`/`run`/`visit_*` and instead makes its own
analyzer pass every dimension on its own merits — which required teaching two
analyzers to see code they were structurally blind to (syn-visitor dispatch and
trait-method cohesion). The `ignore_functions` option is removed. Suppression
markers are also held to a higher bar: targeted pins are orphan-checked against
their own finding-kind, and a metric pin parked too far above the value it
covers is reported.

### Added
- **Structural binary checks are now suppressible by name.** Each structural
  check has its own boolean target — `oi`/`sit`/`deh`/`iet` (coupling) and
  `btc`/`slm`/`nms` (SRP), the lowercased rule code — so a genuinely-unfixable
  finding can be silenced: `// qual:allow(coupling, oi) reason: "orphan rules
  force this impl away from the type"`. A targeted marker silences only its own
  kind, and marking + orphan-detection share one `structural::target_name_for_code`
  mapping. (Before, structural findings could be silenced by *any* targeted
  marker of the dimension via a dimension-only `covers()` check — a silent
  over-suppression — yet had no way to be named deliberately.)
- **Orphan detection is now target-aware, and reports too-loose metric pins.**
  A targeted `// qual:allow(dim, target[=N])` marker is verified against a
  finding of *that exact kind* — a `file_length` pin no longer counts a
  god-struct finding as a match, so a stale targeted marker surfaces even when
  an unrelated finding of the same dimension exists nearby. On top of that, a
  metric pin parked more than `pin_headroom` (default **10%**) above the value
  it covers is reported as an `ORPHAN_SUPPRESSION` — *"too-loose … tighten to
  ~value or remove"* — so a pin can no longer silently absorb regressions up to
  a far-away ceiling. A pin that re-fires (below its value) is left untouched,
  not flagged. New `[suppression].pin_headroom` knob (default `0.10`). Every
  reporter projects the orphan's `kind` (stale vs too-loose) and its targeted
  finding-kind: text/SARIF/GitHub/AI/HTML lead with the status word, and the
  JSON output gains stable `kind` (`"stale"`/`"too_loose"`) and `target`
  (`"file_length=400"`) fields so CI consumers branch on the remedy without
  parsing the message.

### Removed
- **BREAKING: the `ignore_functions` config option is gone.** It excluded
  matching functions from *every* dimension — a far broader effect than its
  stated purpose (papering over call-graph blindness for trait-dispatched
  methods), and the single largest hidden suppression in a project. A
  `rustqual.toml` that still sets `ignore_functions` now fails to parse
  (`deny_unknown_fields`). **Migration:** delete the key. For the rare function
  that genuinely cannot be analyzed, use a targeted `// qual:allow(<dim>, <target>)`
  (or `// qual:allow(iosp)`), `// qual:api`, `// qual:test_helper`, or
  `exclude_files` instead.
- **BREAKING: a bare `// qual:allow(<dim>)` is rejected for any dimension with
  targets** (srp, complexity, dry, coupling, architecture, test_quality). It
  would silence *every* finding of that dimension — too blunt — so it must name
  a target: `allow(srp, god_struct)`, `allow(complexity, max_cyclomatic=20)`,
  `allow(srp, file_length=400)`, … The error lists the valid targets. `iosp`
  (no targets) keeps its bare form; multi-dimension blanket markers
  (`allow(a, b)`) are gone — use one marker per dimension. **Migration:** add the
  target you mean (`rustqual --explain allow` lists them).
- **BREAKING (config): `[srp].file_length_baseline` / `file_length_ceiling` are
  replaced by a single `[srp].file_length`** (default `300`, strict `>`). The
  old baseline→ceiling score ramp was cosmetic (the SRP score is count-based);
  `SRP-002` now simply fires above `file_length`. The matching `[tests]` knob
  is likewise a single `file_length`. **Migration:** a `rustqual.toml` setting
  the removed keys fails to parse (`deny_unknown_fields`) — replace them with
  `file_length`.

### Changed
- **TQ-003 (untested) now models syn-visitor dispatch as real call-graph
  edges.** Previously a visitor's `visit_*` overrides and their helper methods
  looked unreachable from tests (the static call graph can't follow syn's
  `visit_block → visit_expr` dispatch), and the gap was hidden by seeding every
  ignored function as "implicitly tested." That blanket assumption is replaced:
  for each visitor type the helper methods its overrides call are recorded, and
  at each drive-site (`x.visit_block(..)`, `syn::visit::visit_*(&mut x, ..)`,
  `visit_all_files(.., &mut x)`) the driven type is resolved locally and
  `driver → helpers` edges are added. Testedness now flows only when a test
  actually drives the visitor — an undriven visitor is still flagged.
- **SRP cohesion (LCOM4) is more faithful to the canonical metric.** Two fixes,
  both monotonic (they only *merge* components, never create new findings):
  - Non-mechanical trait methods (e.g. a `syn::Visit` impl) now *bridge* the
    inherent methods they tie together — via their field footprint **and** the
    methods they call — without being counted as responsibility nodes. This
    stops a struct whose core logic lives in a behavior trait (visitor, walker)
    from fragmenting into false god-structs. Mechanical traits
    (`Display`/`Debug`/`From`/`Serialize`/`PartialEq`/`Hash`/`Default`/`Clone`/…)
    stay excluded so they can't mask genuine god-structs.
  - Methods connected by a `self.other()` call are now unioned, matching the
    canonical LCOM4 definition (methods cohere when they share a field **or**
    call one another).

### Internal
- `main`, `run`, and all `visit_*` methods are no longer name-ignored; every
  function in rustqual is analyzed by its own rules. The fat AST visitors
  (`Normalizer`, `BodyVisitor`, and several collectors) were refactored to
  per-node category dispatch, and the composition root `run()` was decomposed
  into phase operations — so rustqual dogfoods its own complexity, IOSP, and
  cohesion rules on its own visitor code.
- The two longest files were split into directory modules by concern
  (`shared/normalize/` and `call_parity_rule/calls/`), each well under the SRP
  file-length baseline — eliminating their blanket `allow(srp)` markers by real
  refactoring rather than a pin. rustqual now carries **zero** production `srp`
  suppressions.
- `domain::SuppressionTarget` is now a sum type (`Metric { name, pin } |
  Boolean { name }`) so the "metric ⇒ has a pin, boolean ⇒ has none" invariant
  is unrepresentable otherwise; `suppresses()` matches the variant directly.
  The orphan detector was split into `orphan_suppressions/{mod,positions}.rs`
  (decision vs. enumeration), made target-aware per finding-kind, and fixed so
  a pin on an inactive SRP component, a module-global coupling pin, or a
  magic-number marker is judged correctly rather than mis-reported.

## [1.4.2] - 2026-06-04

Patch release: a false-positive fix found while applying 1.4.1. SIT stops
flagging traits that have `#[cfg(test)]` test doubles. As a deliberate,
documented trade-off it now counts test impls purely by name, so a rare name
collision with an unrelated test-only trait can instead make SIT *under-report*
a genuine single-impl production trait — the safe direction (a missed finding,
never a wrong one). Details and rationale below.

### Fixed
- **SIT (single-impl trait) no longer flags traits with `#[cfg(test)]` test
  doubles.** SIT counted only production impls (metadata collection skips test
  code), so a non-pub trait with one production impl plus test-double impls —
  the idiomatic dependency-injection / test-seam pattern — was wrongly reported
  as an over-abstraction once the test impls became invisible. `#[cfg(test)]`
  impls are now counted toward the trait's total implementor count — whether
  they live in a whole test file, an inline `#[cfg(test)] mod`, or carry
  `#[cfg(test)]` directly on the impl item — and SIT fires only when that total
  is `1`. A genuine single-impl trait (one prod impl, no doubles) is still
  flagged. Counting is deliberately *purely name-based* (the structural metadata
  models no trait identity, on either the production or test side): it never
  tries to resolve which trait a same-named impl targets, which guarantees a real
  test double is never dropped — so a legitimate seam is never re-flagged (no
  false positive, the original bug class). The accepted, documented trade-off is
  a safe false *negative*: an unrelated test-only trait that happens to share a
  production trait's last-segment name is counted toward it and can make SIT
  under-report that production trait. Resolving that needs real trait identity
  (the production side has the same name-collision limitation). Regressed when
  v1.4.x improved
  `#[cfg(test)] mod foo;`-chain test-file recognition (the better classification
  correctly excluded the companion test files, shrinking SIT's denominator). The
  sibling detector OI is unaffected — it matches impl locations, it does not
  count.
- **`#[cfg(test)]` on impls, methods and fns is now honoured by all structural
  detectors.** Teaching SIT to count item-level `#[cfg(test)]` impls surfaced an
  inconsistency: BTC (broken trait contract), SLM (selfless method), NMS
  (needless `&mut self`), DEH (downcast escape hatch) and IET (inconsistent
  error types) still treated a `#[cfg(test)] impl …`, a `#[cfg(test)]` method
  inside a normal impl, or a `#[cfg(test)] pub fn` sitting in a *production* file
  as production code — so e.g. `#[cfg(test)] impl Foo for Mock { fn m(&self) {
  todo!() } }`, or `impl S { #[cfg(test)] fn helper(&self) -> i32 { 42 } }`,
  could still trip BTC / SLM. The detectors now skip `#[cfg(test)]` at every
  level (impl block, impl method, and free fn), consistent with whole test files
  and `#[cfg(test)] mod` blocks (OI/SIT already do, via the shared metadata
  walk).

## [1.4.1] - 2026-06-02

Patch release: **test-suite effectiveness, proven by mutation testing** (Phases
2–4 of the test-quality effort). No change to how the tool scores user code,
except the `prop_assert!` recognition fix below and the removal of the
never-read `[tests].max_methods` config key.

### Fixed
- **TQ-001 no longer flags proptest property tests as assertion-free.**
  `is_assertion_macro` now recognises proptest's `prop_assert!` /
  `prop_assert_eq!` / `prop_assert_ne!` (the `prop_assert` prefix) in addition
  to `assert*` / `debug_assert*`. A property test that asserts only via
  `prop_assert!` is no longer a false TQ_NO_ASSERT.
- **TQ-001 no longer flags `quickcheck!` boolean properties as assertion-free.**
  The macro-expansion pre-pass surfaces `quickcheck! { fn p(x: T) -> bool { … }
  }` as a synthetic `#[test] fn p() -> bool { … }` (params dropped), so a
  property whose body is a bare boolean — e.g. `{ x < 100 }`, with no assertion
  macro or call — was wrongly reported as `TQ_NO_ASSERT`. TQ now treats a
  `-> bool` return as the property's oracle (the value quickcheck checks across
  every input), which a plain `#[test]` can never have. Present since
  macro-body walking shipped in v1.3.2.
- **DEH (downcast escape-hatch) no longer flags `#[test]` function bodies.**
  The detector derived its test-exempt state only from whole-file and
  `#[cfg(test)] mod` context, never a function's own attributes, so a top-level
  `#[test] fn { x.downcast_ref::<…>(); }` was wrongly reported. This surfaced
  via the macro-expansion pre-pass, which emits `quickcheck!` / `proptest!`
  properties as synthetic `#[test]` fns whose original `#[cfg(test)]` wrapper is
  gone — flipping a test body into apparent production code. DEH now honours a
  function's own `#[test]` / `#[cfg(test)]` attribute (consistent with the
  cross-dimension `has_test_attr` rule). The other structural detectors are
  unaffected — they key off impl methods, trait impls, or `pub` signatures, none
  of which a free property fn produces.

### Changed
- **Internal consistency:** the architecture matcher's `has_cfg_test_attr` now
  delegates to the shared `adapters::shared::cfg_test::has_cfg_test` recogniser
  instead of a local copy, so `#[cfg(test)]` detection has a single source of
  truth across all seven dimensions (consistency-audit finding F1).
- **`rustqual --init` templates now include the `[tests]` section.** Both the
  default and the calibrated template previously jumped from `[test_quality]`
  straight to `[weights]`, so generated configs never surfaced the test-code
  threshold knobs (`max_function_lines` / `file_length_baseline` /
  `file_length_ceiling`). They are now emitted commented-out (inherit
  production), matching `rustqual.toml`.
- **Removed the dead `[tests].max_methods` config key.** It was never read, so
  it changed no analysis — the god-struct check (`SRP-001`) already evaluates
  test-file structs at the production `[srp]` thresholds, and a lone per-test
  method-count override (without the other four composite inputs:
  `max_fields` / `max_fan_out` / `lcom4_threshold` / `smell_threshold`) would be
  incoherent. The applied behaviour is unchanged and now made explicit: struct
  SRP-001 deliberately runs on test code (a god-fixture wiring up many concerns
  is a real smell); only the SRP *module*-cohesion (independent-cluster) check
  stays production-only, because a test file's independent `#[test]` fns are its
  purpose. Use `// qual:allow(srp)` for the rare legitimate fixture. `[tests]`
  still configures `max_function_lines` / `file_length_baseline` /
  `file_length_ceiling`. (Note: a `rustqual.toml` that set the removed key now
  errors under `deny_unknown_fields` — delete the line.)

### Tests
- **Mutation-proven coverage** (`cargo-mutants`). The test-recognition core
  (`shared/cfg_test*`) and the structural normalizer (`shared/normalize.rs` +
  `macro_expansion.rs`) now catch every non-equivalent viable mutant; the only
  survivors are documented semantic equivalents. New enumeration tests
  (`normalize_coverage.rs`) pin the exact `NormalizedToken` each operator /
  expression-kind / pattern-kind emits.
- **Mutation sweep extended to the whole analyzer** (Phase 3): all seven
  dimensions plus `app` and `report` are now mutation-proven; every reachable
  survivor was killed, only documented semantic equivalents remain.
- **Per-dimension relevance review** (Phase 4): each dimension's tests were
  audited against the `book/*-quality.md` specs for positive *and* negative
  per-rule coverage, oracle quality (assert the meaningful output, not
  incidentals), and behaviour- vs implementation-coupling. Weak oracles were
  strengthened (e.g. the DRY near-duplicate threshold test, the SRP named-cluster
  assertions, the architecture `PatternScope::accepts` scoping test) and a
  god-fixture-in-a-test-file pin was added for struct SRP-001.
- **proptest** added as a dev-dependency. A path × test-attribute property /
  variant matrix on the recognisers guards against the decentralised
  test-detection bug class that motivated the effort.

### Docs
- **Corrected two spec-vs-code deviations** surfaced by the Phase-4 review:
  `book/function-quality.md` no longer lists `Default::default()` as a `BTC`
  stub form (`is_stub_body` recognises only the `todo!` / `unimplemented!` /
  `panic!("not implemented")` macros), and the `CLAUDE.md` DRY note now matches
  the emitted rule IDs (DRY-003 = duplicate fragment, DRY-004 = wildcard import).

## [1.4.0] - 2026-06-02

Minor release: **quality checks now run on test code.** DRY (duplicate-function
DRY-001, code-fragment DRY-003, repeated-match DRY-005), function-length
(LONG_FN), and SRP file-length (SRP_MODULE) previously skipped
`#[cfg(test)]`/test files. They now analyze test code too, so duplicated test
helpers, copy-pasted arrange/assert blocks, overlong test fns, and oversized
test files are all flagged — at test-specific thresholds that default to the
production values.

### Changed
- **BREAKING (config):** the `[duplicates] ignore_tests` field is **removed**.
  Because `[duplicates]` uses `deny_unknown_fields`, a `rustqual.toml` that
  still sets `ignore_tests` will now fail to parse — delete the line. There is
  no replacement: DRY-001/003/005 always run on tests. `detect_repeated_matches`
  no longer takes a config argument.
- DRY-004 (wildcard imports) remains test-exempt by its own logic, unchanged.
- **LONG_FN now applies to test functions** at the new
  `[tests].max_function_lines` threshold (an `Option` defaulting to
  `[complexity].max_function_lines` = production, 60). Large table-driven tests
  were refactored — case tables hoisted to module-level `const`s so the test
  body stays small; genuinely-long single-scenario tests carry a
  `// qual:allow(complexity)` with rationale.
- **SRP file-length (SRP_MODULE) now applies to test files** at the new
  `[tests].file_length_baseline` / `[tests].file_length_ceiling` thresholds
  (`Option`s defaulting to `[srp]` = 300 / 800). Oversized test files were
  **split along behavioral seams** into focused sub-modules — no suppressions.
  The SRP **cohesion** (independent-cluster) check stays **production-only**: a
  test file's many independent `#[test]` fns are its purpose, not a low-cohesion
  smell.
- Cognitive/cyclomatic/nesting complexity already ran on tests; magic-number
  and error-handling checks remain test-exempt; no change there.

## [1.3.2] - 2026-06-01

Patch release: **test-recognition bug fix** — test functions declared
inside `proptest! { … }` and `quickcheck! { … }` macro bodies are now
visible to every analyzer dimension. `syn` does not expand function-like
macros, so a `#[test] fn` inside `proptest! { … }` was an opaque
`Item::Macro` — its body was invisible to test classification, IOSP, DRY,
complexity, SRP and test-quality alike. A new pre-pass surfaces them.

### Fixed (test detection)

- **`proptest!` / `quickcheck!` bodies are walked.** A new pre-pass
  (`adapters/shared/macro_expansion.rs`, run once at the top of
  `run_analysis`) replaces a recognised test-macro invocation with the
  `fn` items it declares, each marked `#[test]` so it routes through the
  shared `cfg_test::has_test_attr` recognition. The reconstruction is
  lenient: `proptest`'s non-Rust `x in <strategy>` parameter grammar is
  dropped (the body — what length/complexity/DRY measure — is preserved
  verbatim); a leading `#![proptest_config(…)]` inner attribute is
  tolerated; anything that fails to parse is kept as the original opaque
  macro (a blind spot, never a regression). Recognition lives next to
  `cfg_test` in `adapters/shared` to keep all test-recognition policy in
  one place. Failing-first regression tests in
  `src/adapters/shared/tests/macro_expansion.rs`.

### Changed (internal)

- `app::run_analysis` now takes its `parsed` files by value so the
  expansion pre-pass can rewrite them in place before any dimension runs.

## [1.3.1] - 2026-06-01

Patch release: **test-recognition bug fix** — `#[quickcheck]` property
functions are now recognised as test entry points. This continues the
v1.3.0 work of routing all test forms through the shared `cfg_test`
predicates; `quickcheck`'s attribute (path segment `quickcheck`, not
ending in `test`) was the one common framework attribute still missed,
so a `#[quickcheck]` property fn could be flagged `DEAD_CODE` / uncovered
when treated as production code.

### Fixed (test detection)

- **`has_test_attr` (`adapters/shared/cfg_test.rs`) now recognises
  `#[quickcheck]`** alongside `#[test]`, `#[rstest]`, `#[test_case]`, and
  any attribute whose path ends in `test`. Failing-first regression test
  added to `framework_test_attributes_recognized` in
  `src/adapters/shared/tests/cfg_test.rs`.

## [1.3.0] - 2026-05-29

Minor release (one breaking removal — see below): **test-entry-point
recognition routed through one authoritative cfg-test file set** —
framework test attributes and per-crate integration-test directories are
now recognised everywhere, fixing a `DEAD_CODE` false-positive on
`tests/**` integration-test entry points. Package roots are identified by their crate-root file
(`src/lib.rs` / `src/main.rs` / autobinary `src/bin/<name>.rs`) derived
from the parsed `.rs` set — a `tests/` directory only counts when its
owning directory is such a root — and the DRY collectors consume the
shared set instead of their own path strings.
Failing-first regression tests in `src/adapters/shared/tests/cfg_test.rs`,
`src/adapters/shared/tests/cfg_test_files.rs`,
`src/adapters/analyzers/dry/tests/dead_code.rs`,
`src/adapters/analyzers/dry/tests/functions.rs`, and
`src/adapters/analyzers/dry/tests/wildcards.rs`.

### Fixed (test detection)

- **`DEAD_CODE` no longer false-flags integration-test entry points
  under a crate's `tests/` directory.** Two narrow heuristics stacked
  up: (1) `has_test_attr` only recognised the bare `#[test]`, so
  `#[tokio::test]` / `#[rstest]` / `#[test_case]` functions were not
  seen as test roots; (2) the cfg-test file detector classified only
  workspace-root `tests/**`, missing per-crate
  `crates/<name>/tests/**` integration binaries. A `#[tokio::test]`
  entry point in `crates/sv-utility-retry/tests/integration.rs` was
  reported as dead code while the same function in
  `src/**/*_tests.rs` was not. Both narrownesses are removed.

### Changed (unified test recognition)

- **`has_test_attr` (`adapters/shared/cfg_test.rs`) now recognises
  framework test attributes.** Matches any attribute whose path ends in
  `test` (`#[test]`, `#[tokio::test]`, `#[async_std::test]`,
  `#[googletest::test]`, …) plus the renamed macros `#[rstest]` and
  `#[test_case]`. Shared by all seven dimensions, so the broadened
  recognition applies uniformly (IOSP/complexity/DRY/SRP/coupling/TQ/
  architecture leniency for test functions).
- **One authoritative cfg-test file set; the directory rule feeds it and
  nothing else, and is derived from real package roots.**
  `cfg_test_files` first computes the package roots present in the parsed
  tree by their **crate-root file** — `src/lib.rs`, `src/main.rs`, or an
  autobinary `src/bin/<name>.rs` (`""` for the analysis-root crate). This
  is Cargo's defining marker of a package, taken from the `.rs` set with
  no manifest/filesystem access. `cfg_test_files::is_integration_test_path`
  accepts a `tests/` file when the owner of **any** `tests/` component in
  its path is one of those roots (so a package nested under a directory
  literally named `tests`, `fixtures/tests/retry/tests/it.rs`, is matched at
  its real root). This matches `tests/**` and per-crate
  `crates/<name>/tests/**`, but NOT a `tests/` directory whose owner has
  no crate root (`fixtures/tests/**`, `tools/shared/tests/**`, a
  coincidental `src/`+`tests/` pair) nor one nested under a package's
  `src/` (`src/foo/tests/bar.rs`, a unit-test submodule reached via
  `#[cfg(test)] mod`). All four DRY file collectors — `FunctionCollector`
  (duplicate hashing, DRY-001), `FragmentCollector` (repeated fragments,
  DRY-003), `MatchPatternCollector` (repeated matches, DRY-005), and
  `WildcardCollector` (wildcard imports) — no longer apply their own
  `/tests/` path strings; they consult the shared `cfg_test_files` set
  (integration dirs + `#![cfg(test)]` + `#[cfg(test)] mod` chains), so a
  production directory merely *named* `tests` is never blanket-classified
  as test-only, and `ignore_tests` now skips package integration-test and
  `#![cfg(test)]` companion files for every DRY check. Addresses review
  findings: neither `contains("/tests/")` nor a "non-`src` `/tests/`"
  guard actually identified package roots.
  *Known limitation:* a crate using a non-default `[lib] path = …` /
  `[[bin]] path = …` (no file at a conventional `src/` crate-root) is not
  detected as a package root from paths alone — that would require parsing
  `Cargo.toml`.
- **Test Quality coverage checks (TQ-004/TQ-005) key off the
  analyzer-computed `is_test` flag, not a `test_` name prefix.**
  `tq::coverage` previously skipped any function *named* `test_*` via
  an isolated heuristic that both missed framework-attributed tests and
  wrongly exempted production functions named like tests (e.g.
  `test_connection`). It now uses `FunctionAnalysis::is_test`,
  consistent with every other dimension.
- **All structural detectors (BTC, SLM, NMS, IET, OI, SIT, DEH) skip
  whole test files.** They previously skipped only inline `#[cfg(test)]
  mod` blocks (BTC/IET/DEH and the SLM/NMS inherent-method walk) or
  nothing at all — so a stub mock impl, selfless/needless-`&mut`-self
  helper, inconsistent error types, orphaned impl, single-impl trait, or
  `downcast` used in a package integration-test file or `#![cfg(test)]`
  companion was falsely flagged (SRP/Coupling). The visitor-based
  detectors now skip files in `cfg_test_files`; OI/SIT are fixed at the
  source — `collect_metadata` no longer records types/traits/impls from
  test files. Brings the Structural dimension in line with the rest of
  the tool's test-aware behaviour.

### Removed (breaking)

- **`--diff [REF]` changed-files mode removed.** It parsed only the
  git-changed files, so every cross-file analysis ran on a partial set
  and produced false positives/negatives: dead-code reachability (a fn
  used only from an unchanged file looked uncalled), cross-file DRY
  duplicates (one side unchanged → missed), coupling instability (partial
  module graph), architecture call-parity (missing adapters/targets), and
  the cfg-test/package-root detection above. The flag's promise of "the
  same analysis, only reporting changed files" was not what it did — it
  analysed only changed files. Rather than ship a mode that is unsound for
  a whole-program analyzer, it is removed along with `get_git_changed_files`
  / `filter_to_changed`. Run rustqual over the full tree and use
  `--format github` / `sarif` for PR-scoped annotations.

## [1.2.6] - 2026-05-27

Patch release: **dyn-compatibility precision in `must_be_object_safe`** —
the conservative object-safety check used to false-flag legitimate
lifetime-generic methods. Streaming-trait patterns like
`fn stream<'a>(&'a self) -> Box<dyn Iterator + 'a>` (and the
analogous LlmEngine `BoxStream<'a, _>` shape) are dyn-safe per Rust's
actual rule but were rejected by rustqual. Failing-first regression
test in `src/adapters/analyzers/architecture/tests/trait_contract.rs`.

### Fixed (object-safety check)

- **`must_be_object_safe` no longer false-flags lifetime-generic
  methods.** The conservative check in
  `trait_contract_rule/checks.rs::check_object_safety` used
  `!generics.params.is_empty()` to flag method-level generics as
  object-unsafe — but Rust's actual dyn-compatibility rule treats
  lifetime parameters as object-safe (they're compile-time-only and
  erased at codegen; only type and const generics need vtable slots
  the compiler can't synthesise). Affected idiom:
  `fn stream<'a>(&'a self) -> Box<dyn Iterator + 'a>` and any
  streaming-engine trait that ties a returned `BoxStream<'a, …>` to
  `&'a self`. Fix: filter to `GenericParam::Type | GenericParam::Const`
  only via new `has_object_unsafe_generic` helper. Finding message
  sharpened to "has type/const method-level generics". Regression
  tests: `must_be_object_safe_allows_method_level_lifetime` (passes
  for `<'a>`) and `must_be_object_safe_flags_const_generic_method`
  (defensive — locks in that `<const N>` is still flagged).

## [1.2.5] - 2026-05-17

Patch release: **`pub use` re-export resolution gate** — closes a
double-mismatch in trait dispatch and inherent-impl associated-fn
routing where re-exported paths produced graph edges and type-index
keys on the re-export-rooted canonical while the anchor index was
keyed on the declaration canonical. Failing-first regression tests
live in `call_parity_rule/tests/reexport_resolution.rs`.

### Fixed (`pub use` resolution)

- **`pub_fns` sister-site of the gate-threading fix (Codex P2 round 2).**
  The graph collector resolves impl-self-types through the reexport
  map (gate sees `Hidden → private::Hidden`), but the pub-fn surface
  collector was constructing its `CanonScope` with `reexports: None`,
  so it landed on the REEXPORT canonical while the graph produced
  DECL. Check B/D enumerate REEXPORT canonicals against DECL-keyed
  graph reachability → phantom missing-adapter findings on
  `impl crate::application::Hidden { pub fn op() }`-style absolute
  paths against re-exported types. Fix: thread the workspace re-export
  map into `PubFnCollector` via a `Some(&ReexportMap)` field; build
  it inline in `collect_pub_fns_by_layer` via a new
  `build_reexports_for_pub_fns` helper that mirrors the graph-side
  setup. Regression test:
  `pub_fns_impl_self_type_resolves_through_reexport` in
  `tests/reexport_resolution.rs`.
- **Composite `<reexport_canonical>::<method>` callees + REEXPORT-keyed
  `trait_impls` map mismatched DECL-keyed anchor index.** Four sites
  let re-export-rooted canonicals through: `record_trait_impl` (keyed
  `trait_impls` on REEXPORT), `canonicalise_bounds` (built bounds via
  the primitive, bypassing the gate), `is_impl_for_visible_trait`
  (compared REEXPORT vs. DECL), and dispatch edges via
  `canonicalise_generic_param_path` (REEXPORT-rooted). The v1.2.4
  post-pass `rewrite_reexport_edges` only did exact-string-match on
  graph callees, so composite `<x>::<y>` shapes and HashMap keys
  outside `graph.forward` stayed un-normalised.
  Fix: `canonicalise_workspace_path` gains a final re-export-substitution
  step via new `reexports::canonicalise_reexport_path` (longest-prefix
  match) + `apply_reexport_substitution`. `CanonScope`/`ResolveContext`/
  `InferContext`/`FnContext`/`BuildContext`/`WorkspaceIndexInputs` get
  `reexports: Option<&ReexportMap>` propagated from
  `workspace_graph::build_call_graph` (build-order reshuffle:
  `collect_reexport_map` now runs BEFORE `build_workspace_type_index`).
  `signature_params::canonicalise_bounds` switched from
  `canonicalise_type_segments_in_scope` (primitive) to
  `canonicalise_workspace_path` (gate) — closes the last production
  bypass. `reexports::collect_reexport_rewrites` upgraded to
  prefix-aware via `canonicalise_reexport_path` as defense-in-depth.
  Regression tests in `tests/reexport_resolution.rs` cover all four
  external repro shapes
  (`/mnt/d/KI/rustqual-repros/0{1,2,3,4}-*`).
  **Validates** end-to-end against rlm (13 → 0 architecture findings,
  no rlm-side changes).

### Known limitations (documented, not in v1.2.5 scope)

- **Same-name type+value `pub use` collisions can misresolve
  (Codex P2, deferred to WorkspaceCanonical migration).** Valid Rust
  permits `pub use trait_mod::Handler;` (TYPE) and `pub use
  value_mod::Handler;` (VALUE) to coexist — namespaces are separate.
  rustqual collapses both into the same string key in two
  namespace-blind HashMaps:
  1. file-level `AliasMap = HashMap<String, AliasTarget>` (last-write
     wins, so source order determines which entry survives).
  2. workspace `ReexportMap = HashMap<String, String>` (same).
  A trait bound `Q: Handler` can therefore canonicalise to the VALUE
  path and lose its trait-anchor dispatch.
  Codex P2 reproducer kept as `#[ignore]`d regression test
  (`value_reexport_does_not_hijack_trait_bound`); full fix bundled
  into the planned `WorkspaceCanonical { path, namespace }` newtype
  migration where the gate becomes the sole constructor and produces
  canonicals carrying their namespace at the type level.
  See `docs/plan-workspace-canonical-newtype.md` Phase 1.
- **Module re-exports `pub use module_a;`** — `pub use` of a *module*
  (rather than an item) is registered as a leaf-fn re-export in
  `build_reexport_canonical`. A `consumer::module_a::function()` call
  doesn't match the reexport key. Workaround: direct paths. Tracked
  for a follow-up.
- **Glob re-exports `pub use foo::*;`** — explicit known limitation in
  `collect_pub_use_leaves`. Affected users must use direct paths.
  Sketch follow-up plan in `docs/plan-glob-reexport-resolution.md`.
- **`#[path = "..."]` attribute** — `file_to_module_segments` reads
  filesystem path, ignoring `#[path]`. Practical impact null in
  observed projects (cfg-test only). Not in any follow-up plan.

### Future work

- **`WorkspaceCanonical { path, namespace }` newtype migration**
  (`docs/plan-workspace-canonical-newtype.md`) — ~200 sites to
  migrate. Eliminates two drift classes via Rust's type system:
  (a) raw `String` canonicals across HashMaps without explicit
  gate-resolution; (b) namespace-blind `HashMap<String, _>` keys
  silently collapsing same-name type+value entries. Both classes
  surfaced repeatedly in v1.2.4 + v1.2.5 review rounds (leading-colon,
  foo.rs/mod.rs, pub-use composite keys, value/type namespace
  collision). One coherent refactor instead of two separate ones
  because the namespace-marker is a natural field on the newtype.
  ~3-4 weeks incremental work; replaces the reviewer-discipline
  invariant with a compile-time-enforced one.

## [1.2.4] - 2026-05-16

Patch release: **call-parity audit follow-up + post-review
sharpening** — closes the two remaining call-parity gaps surfaced by
re-running the external audit against rustqual 1.2.3, plus four
sibling-resolution / multi-bound findings from the post-1.2.3 review
pass. Failing-first regression tests live in
`call_parity_rule/tests/{module_resolution,target_anchors,type_infer/tests/resolve}.rs`.

### Fixed (sibling-resolution + multi-bound)

- **Tie-break walker leaks (sister-site of the `foo.rs` vs
  `foo/mod.rs` fix).** The shared helper
  `build_module_segs_to_path_map` picked `foo.rs` as the winner, but
  two walkers still iterated the raw `files` slice and let the stale
  `foo/mod.rs` register its own (stale) submodule declarations /
  visibility — re-introducing the false workspace edges the
  tie-break was meant to rule out. Fix: new shared gate
  `forbidden_rule::is_tie_break_winner` that both
  `local_symbols::walk_fallback_roots` and
  `file_visibility::collect_file_root_visibility` route through; the
  loser is skipped (as an implicit fallback root) or marked
  invisible. Also: `build_module_segs_to_path_map` no longer stores
  empty-segs entries — `src/lib.rs` and `src/main.rs` are distinct
  crate roots, not alternatives for the same module identity.
  Regression test:
  `fallback_walker_skips_stale_mod_rs_when_file_rs_is_the_winner` in
  `module_resolution.rs`.
- **`canonicalise_type_segments_in_scope` docstring claimed "Returns
  `None` for external crates".** That was already imprecise before
  the leading-colon fix (the `_ => Some(expanded)` arm returned
  non-crate paths) and became sharper when the new `absolute_root`
  branch added a deliberate extern-path return. Tightened the
  contract to spell out the three return shapes — workspace
  canonical (`Some(["crate", …])`), extern-rooted as-is
  (`Some(other)`), and truly unresolvable (`None`) — and noted that
  workspace-only consumers must filter on `first() == Some("crate")`.
- **Alias map silently dropped `ItemUse.leading_colon`.**
  `gather_alias_map` / `gather_alias_map_scoped` only passed `u.tree`
  into `collect_alias_entries`, so `use ::ext::Foo as Local;` was
  stored byte-equivalent to `use ext::Foo as Local;`. At the use site
  `Local::method()`, `normalize_after_alias` then happily prepended
  `crate::` once `ext` matched a workspace top-level module, creating
  a false workspace edge to a symbol the program never addressed.
  Fix: `AliasMap` value is now `AliasTarget { segments,
  absolute_root: bool }` — `collect_alias_entries` reads the parent
  `ItemUse.leading_colon` once and stamps every entry; the new gate
  short-circuits in `normalize_after_alias`. Single ownership of the
  flag end-to-end so future drift between extraction and use-site is
  compiler-prevented. Regression test:
  `absolute_use_alias_does_not_re_canonicalise_to_workspace` in
  `module_resolution.rs`.
- **`src/foo.rs` and `src/foo/mod.rs` collision was order-dependent.**
  `collect_workspace_module_paths` and `collect_file_root_visibility`
  both built a `module-segments → file-path` index via
  `.collect()` into a HashMap. When both files exist (stale-leftover
  refactor), `file_to_module_segments` mapped them to the same key
  `["foo"]` and whichever entry landed last in iteration order
  silently overwrote the other — the walker then descended into the
  wrong AST, producing non-deterministic `workspace_module_paths`
  and missed call edges. Fix: new shared helper
  `forbidden_rule::build_module_segs_to_path_map` applies Rust's
  modern precedence rule (prefer `foo.rs` over `foo/mod.rs`); both
  collectors route through it so the tie-break lives in one place.
  Regression test:
  `file_rs_wins_over_dir_mod_rs_when_both_back_the_same_module_path`
  in `module_resolution.rs`.
- **Sibling submodule loses to crate-root with same name.**
  `normalize_after_alias` checked `crate_root_modules` before the
  sibling-submodule branch, mis-routing `use foo::X` inside
  `crate::application` (where both `crate::application::foo` and
  `crate::foo` exist) to the crate-root module. Fix: reorder the
  match arms — local sibling check fires first, matching Rust 2018+
  resolution priority. Regression test:
  `sibling_submodule_wins_over_crate_root_with_same_leaf_name` in
  `module_resolution.rs`.
- **Inline mods invisible to sibling discriminator.**
  `collect_workspace_module_paths` collected only file-backed paths,
  leaving inline `mod foo { mod bar { ... } use bar::X }` patterns
  with no entry for `[foo, bar]`. Fix: walk each file's AST for
  `Item::Mod` blocks (cfg-test skipped) and add their paths.
  Regression test: `inline_mod_sibling_import_traces_edge` in
  `module_resolution.rs`.
- **Multi-bound generic receiver drops later bounds.**
  `q.method()` where `q: &Q` and `Q: T1 + T2`: the receiver path
  returned only the first bound, so if the method lived on `T2`,
  `trait_dispatch_edges` filtered the first bound out and never
  tried the second. Fix: `CanonicalType::TraitBound` payload
  extended from `Vec<String>` to `Vec<Vec<String>>` (one entry per
  bound). All three multi-bound TRAIT spellings (`dyn T1 + T2`,
  `impl T1 + T2`, `<Q: T1 + T2>`) now collect every resolvable
  trait bound; `canonical_edges_for_method` fans out one edge per
  bound; `lookup_trait_method_return` finds the first bound that
  defines the method. UFCS and receiver paths are now symmetric
  for trait-bound dispatch. **Remaining limit:** a `Future<Output =
  T>` bound still short-circuits to `CanonicalType::Future` and
  drops any peer trait bounds — `CanonicalType` can't carry both a
  Future and a TraitBound simultaneously, so
  `impl Future<Output = T> + Handler` is still resolved as `Future`
  only. Regression test:
  `multi_bound_generic_receiver_method_call_emits_anchor_for_defining_bound`
  in `target_anchors.rs`.
- **Orphan files faked as sibling submodules.**
  `collect_workspace_module_paths` derived its set from raw file
  paths plus inline `mod` blocks, with no check that the parent
  module actually declared the file. A stale file like
  `src/application/orphan.rs` (no `pub mod orphan;` in
  `application/mod.rs`) would still contribute
  `["application", "orphan"]` to the lookup, making
  `use orphan::T` from a sibling look like a local sibling import
  and fabricating an edge to `crate::application::orphan::T` — a
  canonical that isn't in the call graph. Fix: only files
  reachable from a crate root via declared `mod` edges contribute
  paths. Orphan files are unreachable in that walk and so stay
  out of the lookup. cfg-test mods are skipped uniformly across
  file-backed and inline declarations. Regression test:
  `orphan_file_does_not_act_as_sibling_submodule` in
  `module_resolution.rs`. (The initial 1.2.4 fix wired the lookup
  through `file_root_visibility`; the private-mod fix below replaced
  that with a declared-edge walk that handles both invariants in
  one pass — the description here reflects the final shape.)
- **Private `mod foo;` dropped from sibling-submodule lookup when
  ancestor chain was hidden.**
  The initial orphan-file fix wired `collect_workspace_module_paths`
  through `file_root_visibility`, which is the right filter for
  public-surface decisions but too narrow for module membership.
  Inside a subtree hidden by a non-root private `mod hidden;`,
  every child file inherited `visibility=false` and its own
  `mod response;` declarations stopped contributing paths — even
  though, from code INSIDE the hidden subtree, `use response::T`
  must still resolve to `crate::…::hidden::response::T`. Fix:
  drop the visibility filter for module-membership collection.
  Walk from crate roots through every declared `mod X` (file-
  backed and inline, any visibility), skipping cfg-test mods.
  Orphan exclusion (the prior invariant) is preserved because
  unreachable files are never visited by the walker. Regression
  test:
  `private_mod_sibling_import_resolves_even_when_ancestor_chain_is_hidden`
  in `module_resolution.rs`.
- **Unbounded generic param could collide with same-named workspace
  symbol.** `resolve_generic_path` short-circuited to a trait bound
  only when the fn-scoped generic param had at least one non-empty
  bound; an unbounded `<Q>` fell through to normal canonicalisation,
  which could resolve `Q` to a workspace type/module of the same
  name and produce wrong method-dispatch edges. Fix: extracted a
  `generic_param_shadow` helper that classifies the match into three
  states (bounded → `GenericParamBound` carrying the canonical
  bounds, unbounded → `Opaque` shadow, not-a-param → fall through)
  so an unbounded match shadows the workspace lookup with `Opaque`.
  Regression test:
  `unbounded_generic_param_shadows_same_named_workspace_symbol` in
  `type_infer/tests/resolve.rs`. (Initially introduced as the new
  variant `TraitBound`; later split into a dedicated
  `GenericParamBound` variant when the same-named conflation with
  `impl Trait` / `dyn Trait` returns surfaced — see "Split
  `CanonicalType::TraitBound`" below.)
- **Workspace type index ignored fn / method / impl / struct
  generics.** The shadowing fix above only fires when
  `ResolveContext.generic_params` is populated, but the
  `workspace_index/{functions,methods,fields}.rs` collectors all
  constructed their `ResolveContext` via a helper that hard-coded
  `generic_params: None`. So `pub struct Q;` plus any of
  `pub fn get<Q>() -> Q`,
  `impl<Q> Service<Q> { fn first(&self) -> Q }`, or
  `pub struct Container<Q> { item: Q }` would index the return / field
  as the workspace struct `Q`, poisoning later inference — e.g.
  `get::<Session>().diff()` could short-circuit on the wrong concrete
  return instead of using the turbofish. Fix: threaded the fn's,
  impl's, and struct's canonical generics (via the unified
  `signature_params::item_canonical_generics` /
  `method_canonical_generics` helpers — see "Unified generics-
  canonicalisation pipeline" below) into the per-collector resolve
  context across all three indexing surfaces.
  Audit covered every `ResolveContext` construction in the crate;
  the alias-body case in `resolve_alias::expand_alias` correctly
  keeps `generic_params: None` because alias bodies live at the
  alias decl-site, not the use-site fn. Regression tests:
  `fn_generic_param_return_does_not_collide_with_same_named_workspace_type`,
  `method_generic_param_return_does_not_collide_with_same_named_workspace_type`,
  `impl_level_generic_param_return_does_not_collide_with_same_named_workspace_type`,
  `struct_generic_param_field_does_not_collide_with_same_named_workspace_type`
  in `type_infer/tests/workspace_index.rs`.
- **Workspace-index generic bounds were not canonicalised.** The
  first version of the workspace-index generics threading (above)
  stored raw bound segments (e.g. `[["Handler"]]`) —
  `generic_param_shadow` in `resolve.rs` then wrapped those into the
  bound variant literally, so downstream `trait_has_method` /
  anchor-index lookups (keyed on
  `crate::ports::handler::Handler`) silently missed and trait
  dispatch on a `Q: Handler` return dropped valid edges. The body
  collector already had a canonicaliser; it has been lifted from
  `calls.rs` into `signature_params.rs`, and the workspace-index
  collectors now route their generics through it before constructing
  the resolve context. (Subsequently folded into the unified
  `item_canonical_generics` / `method_canonical_generics` helpers —
  see "Unified generics-canonicalisation pipeline" below.) Regression
  test:
  `bounded_fn_generic_param_return_carries_canonicalised_trait_bound`.
- **Bounded generic-param return blocked turbofish inference.**
  Once `fn get<Q: Handler>() -> Q` could be indexed with the bound
  available, `infer_call` (which tried `fn_returns` before any
  turbofish fallback) used the bound and never reached the
  turbofish — so `get::<Session>().diff()` lost the inherent
  `Session::diff` edge. Fix: when the index returns a
  `GenericParamBound` AND the call site carries an explicit
  turbofish, the turbofish wins (it's strictly more specific than
  the param's bound). Plain bare-call sites still get the bound so
  trait-anchor dispatch on the return value keeps working.
  Regression test:
  `bounded_fn_generic_param_return_does_not_block_turbofish_inference`.
- **Method-return indexing missed method-level where-bounds on
  impl-level generics.** `methods.rs` used a method-only generics
  extractor that ignored `where Q: T` when `Q` was an impl-level
  (not method-level) generic. The body collector had already needed
  to thread outer names through the method's where clause for this
  exact shape (`impl<Q> Service<Q> { fn current(&self) -> Q where
  Q: Handler }`). `methods.rs` now goes through the unified
  `method_canonical_generics(sig, impl_generics, …)` helper which
  handles the outer-name-extending pass. Regression test:
  `method_generic_param_return_canonicalises_where_bound_on_impl_generic`.

### Changed (architectural)

- **Unified generics-canonicalisation pipeline.** The body collector
  (`file_fn_collector.rs` / `calls.rs`) and the three workspace-index
  collectors (`workspace_index/{functions,methods,fields}.rs`) all
  composed the same three-step pipeline (extract → merge →
  canonicalise) manually. That duplication was the root cause of
  the three findings above — each site missed a different
  intermediate step. The pipeline now lives behind two public
  helpers in `signature_params.rs`:
  - `item_canonical_generics(generics, file, mod_stack)` — for free
    fns, structs, impl blocks (no outer scope).
  - `method_canonical_generics(sig, impl_generics, file, mod_stack)` —
    for methods inside `impl` blocks (merges impl-level + method-
    level + canonicalises).
  The atomic helpers (`extract_*`, `canonicalise_bounds`) are private
  to `signature_params.rs`. External callers can no longer compose
  them incorrectly; the only way to construct a canonical generics
  map is via one of the two public entry points.
  `FnContext.generic_params` type changed from
  `Vec<(String, Vec<Vec<String>>)>` (raw) to
  `HashMap<String, ParamInfo>` (canonical + turbofish-position
  tagged) — callers MUST
  build it via the helpers.
- **Unified turbofish-override across path-call and method-call.**
  The "turbofish wins over a bounded-generic-param return" rule lived
  inline in `infer_call` but not in `infer_method_call` — an
  asymmetry that would have silently dropped
  `s.method::<Session>().diff()` edges when method-call turbofish
  meets a generic-param method return. Both call shapes now share
  `turbofish_substitute(inferred, turbofish, ctx)` in
  `infer/generics.rs`. The free-fn-only `turbofish_fallback(...)`
  remains separate by design: an unindexed method on an Opaque
  receiver gives no signal that the turbofish substitution is
  meaningful, so method calls would otherwise synthesise false
  `Session::method()` edges whenever the receiver type is unknown.
  Regression tests:
  `method_call_turbofish_overrides_bounded_generic_param_return`,
  `free_fn_impl_trait_return_turbofish_does_not_substitute_return`,
  `method_call_impl_trait_return_turbofish_does_not_substitute_return`.
- **Split `CanonicalType::TraitBound` into two variants.**
  The previous single `TraitBound` variant conflated two semantically
  distinct return shapes that look the same in the index: (a) bare
  fn-generic-param ident return (`fn f<Q: T>() -> Q` where `Q` IS
  the return) — turbofish substitution at the call site CAN replace
  it; (b) `impl Trait` / `dyn Trait` return — the return type is
  opaque even when bounds are known; turbofish on OTHER generic
  params doesn't substitute it. `turbofish_substitute` fired on both
  and produced false `Session::diff` edges for
  `fn make<T>() -> impl Handler` + `make::<Session>().diff()`. Fix:
  new `CanonicalType::GenericParamBound` variant produced exclusively
  by `generic_param_shadow` (case a); `TraitBound` stays for case b.
  Both variants dispatch identically through the trait-anchor index
  (every consumer that used to match `TraitBound` now goes through
  `CanonicalType::as_trait_bounds()` which handles both). Only
  `turbofish_substitute` distinguishes them — fires only for
  `GenericParamBound`.
- **Turbofish substitution lost which generic param position is
  returned, and didn't recurse through wrapper returns.**
  `turbofish_substitute` always picked the FIRST turbofish arg, so
  `fn get<A, Q: Handler>() -> Q` called as `get::<Audit, Session>()`
  would substitute `Audit` (position 0) instead of `Session`
  (Q's position 1). And the substitution only fired when the WHOLE
  inferred return was `GenericParamBound` — wrapper returns like
  `fn get<Q: Handler>() -> Result<Q, E>` left the inner `Q`
  un-substituted, so `get::<Session>().unwrap().diff()` lost the
  Session::diff edge after `.unwrap()` peeled the Result. Fix:
  `GenericParamBound` now carries `turbofish_index: Option<usize>`,
  populated at index-build time by
  `signature_params::item_canonical_generics` /
  `method_canonical_generics` based on the param's position in the
  callee's substitutable generics list (method-own params for
  methods, fn-own params for free fns; impl-level params get `None`
  because they're determined by the receiver type, not the method-
  call turbofish). `turbofish_substitute` now uses the index to pick
  the correct turbofish arg AND recurses through `Result` /
  `Option` / `Future` wrappers so the inner `Q` substitutes before
  combinator-unwrap peels the wrapper. The atomic
  `signature_params::ParamInfo { bounds, turbofish_index }` struct
  replaces the old `Vec<Vec<String>>` value type in
  `FnContext.generic_params` / `ResolveContext.generic_params` /
  `InferContext.generic_params`. Regression tests:
  `multi_generic_fn_turbofish_picks_correct_arg_for_returned_param`,
  `wrapper_around_generic_param_return_substitutes_inner_via_turbofish`,
  `method_call_wrapper_around_generic_param_return_substitutes_inner`.
- **Turbofish substitution missed `Vec<Q>` / `HashMap<_, Q>` wrappers
  and shadowed explicit absolute paths.** Two sister gaps from the
  same review pass:
  - `turbofish_substitute` recursed through `Result` / `Option` /
    `Future` but not through `Slice` (`Vec<Q>` → `Slice(Q)` in the
    resolver) or `Map` (`HashMap<K, Q>` → `Map(Q)`). So
    `for s in get::<Session>() { s.diff(); }` for
    `fn get<Q: Handler>() -> Vec<Q>` left the inner Q un-substituted
    and the iterator binding's `s` stayed as `GenericParamBound`,
    losing the `Session::diff` edge. Fix: added `Slice` / `Map` arms
    to the recursion. Regression test:
    `vec_around_generic_param_return_substitutes_inner_via_turbofish`.
  - `generic_param_shadow` matched purely on segment text, so an
    explicit absolute path `::Q::method()` (Rust 2018+ extern-crate
    root) was mis-shadowed by an in-scope generic param named `Q`.
    Initial fix targeted only the type resolver (`resolve_generic_path`).
    The CALL collector (`visit_expr_call` →
    `canonicalise_generic_param_path` in `calls.rs`) had a parallel
    generic-param lookup that ALSO ignored `leading_colon`. Unified
    via `signature_params::matched_generic_param(segments,
    leading_colon_set, generic_params)` as the single gate. Both
    `generic_param_shadow` (type path) and
    `canonicalise_generic_param_path` (call path) route through it.
    Direct `generic_params.get(name)` lookups in any new code now
    bypass the gate — code review / D-sweep grep for that pattern
    catches new drift candidates. Regression tests:
    `absolute_leading_colon_path_is_not_shadowed_by_in_scope_generic`,
    `absolute_leading_colon_call_path_does_not_shadow_to_trait_anchor`.
  - Even after the generic-param gate, both fallback paths still
    dropped `leading_colon` when consulting the workspace
    canonicalisation primitive — `::Q` with a workspace-local `Q`
    would resolve to `crate::...::Q` (false workspace edge). Added
    `bindings::canonicalise_workspace_path(segments, leading_colon_set,
    scope)` as the central use-site canonicalisation gate (wraps
    `canonicalise_type_segments_in_scope` with the leading-colon
    short-circuit). `resolve_generic_path` (type) and
    `canonicalise_call_path` in `infer/call.rs` (inference) both
    route through it. The call collector's `canonicalise_path`
    (which doesn't go through the primitive) gets an inline
    early-return at the top — same gate semantic, locally enforced.
    Regression tests:
    `absolute_leading_colon_type_path_does_not_route_to_same_named_workspace_type`,
    `absolute_leading_colon_call_path_does_not_route_to_same_named_workspace_fn`.
  - Trait bounds had the same leak through TWO additional sites:
    `signature_params::trait_bound_paths` stripped `leading_colon`
    at extraction time (so `<Q: ::ext::Trait>` stored bare segments
    that then canonicalised to a same-named workspace trait), and
    `type_infer::resolve_bound_list` (for `dyn Trait` / `impl Trait`
    types) called `canonicalise_type_segments_in_scope` directly
    without `leading_colon`. Fix: `trait_bound_paths` filters out
    leading-colon bounds at extraction; `resolve_bound_list` routes
    per-bound through `canonicalise_workspace_path`. Regression
    tests:
    `generic_param_bound_with_leading_colon_does_not_route_to_workspace_trait`,
    `dyn_trait_with_leading_colon_does_not_route_to_workspace_trait_anchor`.
  - Final sweep — all other direct callers of
    `canonicalise_type_segments_in_scope` that take a `syn::Path`
    were converted to `canonicalise_workspace_path`:
    `binding_type_from_init` (let-binding ctors),
    `canonical_from_type` (legacy type-annotation resolver),
    `workspace_index/traits::resolve_trait_path` (`impl Trait for X`
    self-types), `is_impl_for_visible_trait` (pub-surface visibility),
    `is_transparent_wrapper_path` (pub-fn-visibility marker check),
    `resolve_alias_target_canonical` (alias-chain follow-through),
    `resolve_use_source_type` + `register_pub_use_leaves` (`use` /
    `pub use` resolution — `leading_colon` threaded via `UseTreeCtx`
    and an extra param on the leaf registrar),
    `workspace_graph::resolve_impl_self_type` (impl self-types),
    `is_marker_trait` + `identify_wrapper_name` (stdlib markers /
    wrappers — extern-rooted paths no longer mis-match same-named
    workspace traits). The only remaining direct callers of the
    primitive are the wrapper itself, the legacy flat-map adapter
    (which gates inline), and `canonicalise_bounds` (whose inputs
    are filtered by `trait_bound_paths` at extraction).

### Performance

- `local_symbols::has_workspace_ancestor` no longer allocates a
  fresh `Vec<String>` per prefix probe (`base[..len].to_vec()` →
  `&base[..len] as &[String]`). The `Vec<T>: Borrow<[T]>` impl lets
  `HashMap::contains_key` take the slice directly. Hot path on
  large workspaces; flamegraph surfaced the transient allocation.

### Fixed

- **Class 1 (call-parity / receiver-position trait dispatch)** —
  `query.execute(args)` where `query: &Q` and `Q: Trait` now emits
  the trait-method anchor edge, matching the UFCS form
  (`Q::execute(args)`) shipped in 1.2.3. Round-4's
  `canonicalise_generic_param_path` only fired for `Expr::Call`
  with `Expr::Path`, missing the `Expr::MethodCall` path entirely
  — 5th spelling overlooked in the cross-product enumeration. Fix
  is upstream in `resolve_type` itself: a new optional
  `generic_params` field on `ResolveContext` + `InferContext` lets
  the path resolver recognise a single-segment ident that names a
  fn-scoped generic param and return `CanonicalType::TraitBound`
  for it. The existing TraitBound → `trait_dispatch_edges` →
  anchor pipeline picks up the seeded binding without any
  parallel logic. Every downstream consumer of `resolve_type`
  (signature seeding, let-binding inference, return-type chasing,
  closure-arg seeding) benefits at one stroke. Regression tests:
  `method_call_on_generic_receiver_emits_trait_anchor_edge`,
  `method_call_on_where_clause_bound_generic_emits_trait_anchor_edge`,
  `method_call_on_impl_level_generic_emits_trait_anchor_edge`
  in `target_anchors.rs`.
- **Class 2 (call-parity / sibling-submodule UFCS)** —
  `Type::new(args)` where `Type` is imported via
  `use submodule::Type;` (relative sibling-submodule) now traces
  correctly. Pre-existing latent gap (1.2.2 reproduces same
  finding); newly visible in 1.2.3 because the Bug-2 `pub use`
  fix made the enclosing generic dispatcher reachable, surfacing
  the inner gap. Root cause: `normalize_after_alias` returned
  relative paths (`["response", "Type", "new"]`) as-is when the
  first segment wasn't in `crate_root_modules`, while the
  recorded node canonical was absolute
  (`["crate", "application", "response", "Type", "new"]`). Edge
  pointed at a phantom node → no graph edge. Fix adds a new
  `workspace_module_paths: HashSet<Vec<String>>` set on
  `FileScope`, derived from every workspace file's
  `file_to_module_segments`. The `_` arm of `normalize_after_alias`
  now distinguishes sibling-submodule imports from extern-crate
  imports by checking whether the would-be-absolute prefix
  matches a known workspace module path — sibling submodules get
  the implicit-self relative resolution (Rust 2018+ language
  rule); extern crates preserve their absolute extern-path shape
  so `is_stdlib_prefixed` and `resolve_bound_list`'s "first ==
  'crate'" gate keep their existing behaviour. Regression test:
  `concrete_ufcs_via_sibling_submodule_import_traces_edge` in
  `call_parity_rule/tests/module_resolution.rs`.

## [1.2.3] - 2026-05-14

Patch release: **call_parity audit fixes** — closes five gaps surfaced
by an external rmcp-based project's call-parity audit. Each fix is
reproduced by a failing-first regression test in
`call_parity_rule/tests/{receiver_tracing,module_resolution,target_anchors,check_b}.rs`;
all tests + self-analysis stay 100% green after the fixes land.

- **Bug 1 (call-parity / rmcp `#[tool_router]`)** — proc-macros
  generate the public dispatch surface at expansion time around a
  user-written `async fn` that's syntactically private. rustqual
  reads pre-expansion source, so it filtered such methods out of
  the adapter-handler enumeration and any application call they
  reached appeared as "not reached from adapter X". New config
  `[architecture.call_parity] promoted_attributes` (default empty,
  pure opt-in) lifts a private fn onto the handler surface when it
  carries a matching attribute. Implementation in `pub_fns.rs`.
- **Bug 1 / discoverable hint** — every `CallParityMissingAdapter`
  finding now carries an optional hint that points at private
  attributed fn(s) in the missing adapter that would resolve the
  finding if their attribute were promoted. Candidates are
  filtered on missing-adapter membership, syntactic privacy,
  non-stdlib attribute, visible enclosing-mod chain, and visible
  impl self-type; reachability is verified by reusing the
  production `compute_touchpoints` walker (treating the candidate
  as if it were a handler) so `call_depth`, peer-adapter, and
  boundary-stop semantics are applied identically — a hint
  appears iff promoting the attribute would actually put the
  target into the adapter's coverage. Embedded in the
  `Finding.message` text so all output formats (text / JSON /
  SARIF / GitHub / AI / findings_list) surface it without
  per-format plumbing. New module `hint/`.
- **Bug 2 (call-parity / `pub use` re-exports)** — caller writing
  `middleware::record_operation()` against a `pub use
  savings_recorder::record_operation` re-export saw the edge dropped
  because the canonical mapped to a path with no fn definition. New
  workspace-wide reexport map rewrites callee canonicals to their
  real definitions in a post-pass. New module `reexports.rs`; shared
  `apply_edge_rewrite` helper in `workspace_graph/edge_rewrite.rs`.
- **Bug 3 (call-parity / cascade orphans)** — fns reachable only
  through the trait-anchor chain (e.g. `record_query::<Q> →
  Q::execute → impl_method → helper`) appeared as orphan targets.
  `build_adapter_reachable_targets` now propagates anchor → impl
  methods so the BFS reaches the impl body and its target-internal
  callees. Resolves automatically once Bug 4 emits the anchor edge.
- **Bug 4 (call-parity / generic trait dispatch)** — `Q::method(...)`
  where `Q: Trait` produced `<bare>:Q::method` instead of an edge to
  the trait anchor `<Trait>::method`. New
  `extract_method_generic_params` helper threads trait-bound info
  from the fn signature into the call collector; a new branch in
  `canonicalise_path` emits one edge per bound, riding on the
  existing anchor machinery. All three bound spellings are
  recognised: inline (`fn f<Q: Trait>`), method-level where
  (`fn f<Q>(...) where Q: Trait`), and method-level where on an
  impl-level generic
  (`impl<Q> Foo<Q> { fn f(&self) where Q: Trait }`) — the last
  case is handled by extending the method's where-bounds against
  the impl-level generic names so the predicate isn't dropped.
- **Bug 5 (suppression / typo silencing)** — `// qual:allow(srp_params)`
  (or any unknown-dimension form, including bare `// qual:allow`
  without parens) silently parsed to a zero-dim `Suppression` and
  was treated as global suppress. Typos hid every finding category
  on the annotated function. The parser now returns `None` for
  empty / unrecognized dimension lists. To keep typos auditable
  rather than silently dropping them, `detect_invalid_qual_allow`
  flags `// qual:allow(<unknown>)` forms (parens with content but
  no recognised dimension) and any unclosed-paren form
  (`// qual:allow(iosp`, `// qual:allow(srp_params`, …) regardless
  of whether the tail spells a valid dimension — structural
  malformation always surfaces. A separate side-channel
  (`collect_invalid_qual_allow_lines`) projects them directly to
  `ORPHAN_SUPPRESSION` findings, bypassing the suppression-application
  pipeline so the marker can never accidentally suppress real
  findings via the empty-dimensions wildcard semantic. Bare
  `// qual:allow` and `// qual:allow()` carry no intent and are
  silently ignored.
- **Hint precision (post-Codex review)** — three precision
  refinements to `hint`: (a) `CandidateCollector` requires
  `Visibility::Inherited` explicitly so `pub` fns excluded from
  `pub_fns_by_layer` for other reasons (private mod chain) aren't
  flagged as promotion candidates; (b) impl-self-type
  canonicalisation goes through `resolve_impl_self_type` (matching
  `pub_fns` / `file_fn_collector`) so candidates in `impl
  super::Server` / `use ...; impl Server` shapes intersect the
  workspace graph correctly; (c) `cfg_test_files` filter applied at
  the per-file iteration level so test-only fns can't surface as
  hint candidates.

## [1.2.2] - 2026-05-13

Patch release: **Reporter-Trait sealed two-trait + Snapshot pattern**,
**Call-parity anchor model + Orphan-suppression in trait contract**.

Late-cycle additions (post 2026-04-30 tag):

Round 1 — Anchor model, unified target-capability rule (Codex 2026-05-04, P1-P4):

- **Single rule for walker + Check B/D**: `is_anchor_target_capability`
  in `anchor_index` is the only source of truth for "is this anchor
  a target capability". Walker (`is_target_boundary`) and Check B/D
  (`target_anchor_capabilities`) share it; previously each side
  re-implemented the rule and drifted (parallel-path inconsistency).
  Rule: anchor passes iff (a) declaring layer is NOT a peer adapter,
  AND (b) declaring layer IS the target with a callable body
  (default OR overriding impl), OR at least one overriding impl
  lives in the target layer.
- **Peer-adapter anchor rejection** (P2): anchors whose declaring
  trait lives in a configured peer-adapter layer are excluded from
  target capabilities. Prevents `cli` from inheriting `mcp::Handler`
  coverage via the anchor side-channel.
- **Default-only target-layer anchors** (P3): trait declared in
  target with a default body and no overriding impls is now
  enumerated as a capability — Check A used to accept the touchpoint
  (anchor in target layer) while Check B/D refused to enumerate
  ("no overriding impl"). Pure-signature trait methods (no default,
  no impl) stay rejected (uncallable).
- **Concrete impl-method skip in Check B/D** (P1): when an anchor is
  enumerated as target capability, its overriding impl-method
  canonicals (`<Impl>::<method>`) are skipped in the concrete
  pub-fn iteration **only when no adapter has the concrete in its
  coverage**. If at least one adapter calls the concrete directly
  (`LoggingHandler::handle()` UFCS or static-method form) while
  another adapter dispatches via `dyn Trait`, the concrete pass
  still runs — the mixed-form drift then surfaces as a concrete
  finding plus an anchor finding for the adapter that uses the
  other form. Cross-form synonym handling stays intentionally out
  of scope; without the gating refinement, mixed-form drift was
  silently masked behind a single false-positive anchor-orphan.
  Same conditional skip is mirrored in Check D (`check_d::check_multiplicity_mismatch`)
  via `any_adapter_counts_concrete` — without it, all-direct-call
  multiplicity drift (cli=2 vs mcp=1, both calling concrete
  directly with no dispatch) was silently dropped because Check D's
  `is_anchor_backed_concrete` skip ran unconditionally.
- **Anchor findings carry real source line** (P4): `AnchorInfo` now
  stores the trait method's source location captured at
  type-index-build time (`MethodLocation { file, line, column }`).
  Anchor findings (Check B `CallParityMissingAdapter`, Check D
  `CallParityMultiplicityMismatch`) report the trait method's
  declaration line instead of `line: 0`. Suppression-window
  matching, the orphan detector's window scan, and SARIF
  `startLine` validity all work for anchor-level findings.

Round 2 review (Codex 2026-05-04):

- **Anchor-only target surface defensive guard** (P1): Check B's
  early-return on missing target-layer entry in `pub_fns_by_layer`
  is replaced with an empty-slice fallback. The target-anchor
  enumeration runs unconditionally. Empirical workspaces always
  carry an entry (the pub-fn collector's `or_default()` ensures it),
  but the fallback locks in the invariant against future refactors —
  an anchor-only target surface (e.g. ports trait impl'd by a
  private application type, or default-only trait declared in
  target) cannot silently lose missing-adapter findings.
- **Reachable-target BFS recognises trait anchors** (P2):
  `build_adapter_reachable_targets` now treats a callee as a
  target-capability node when EITHER its resolved layer matches
  `target_layer` OR it is a synthetic anchor that passes
  `is_anchor_target_capability` for `(target_layer, adapter_layers)`.
  Previously, an anchor reached transitively via an adapter-touched
  target fn (adapter → target fn → `dyn Trait.method()`) was
  invisible to the BFS (anchor's `layer_of()` is the trait
  declaration layer, e.g. `ports`), and Check B fired a false
  orphan. Post-boundary plumbing wired up via at least one adapter
  now stays silent for trait anchors too.
- **cfg-test trait method filter** (P2): per-method `#[cfg(test)]`
  / `#[test]` attributes inside an otherwise-production trait now
  exclude the method from `WorkspaceTypeIndex.trait_methods`,
  `trait_method_locations`, and `trait_methods_with_default_body`.
  Without this, a `#[cfg(test)] fn helper(&self) {}` with a default
  body would promote the method to a target anchor capability
  even though it is invisible in production builds, and
  `trait_has_method` would accept dispatch calls that should stay
  unresolved.

Round 3 review (Codex 2026-05-04):

- **Private trait anchor exclusion** (P1): `WorkspaceTypeIndex` now
  captures the trait declaration's effective workspace visibility in
  `trait_visibility: HashMap<String, bool>`, threaded into
  `AnchorInfo.trait_visible`, and consulted as a precondition by
  `is_anchor_target_capability`. Without this, `trait Internal { fn
  run(&self) {} }` (no `pub`) and `trait Hidden { fn run(&self); }
  impl Hidden for X` (private trait + target impl) surfaced as
  Check B/D capabilities and produced orphan findings for what is
  architecturally implementation detail. Effective visibility is the
  trait's own `vis == Public` ANDed with the trait collector's
  `enclosing_mod_visible` (mirroring `pub_fns::PubFnCollector`'s mod
  visibility tracking) — so a `pub trait T { … }` declared inside a
  private `mod inner { … }` is also rejected, since it isn't
  reachable from outside its own module and thus isn't part of the
  architectural surface.
- **Anchor orphan suppression for direct-concrete coverage** (P1):
  `check_b::inspect_anchor` adds a second arm to the
  `reached.is_empty()` suppression: when at least one of
  `info.impl_method_canonicals` is in some adapter's coverage or in
  the reachable set, the anchor finding is silenced. Closes the
  all-direct-concrete false-positive — every adapter calls
  `LoggingHandler::handle()` via UFCS, none dispatches via
  `dyn Trait`, the concrete pass is silent (all reach concrete),
  and the anchor pass no longer fires "missing all adapters" since
  the concrete coverage IS the capability coverage.
- **`exclude_targets` matches impl path on anchor findings** (P2):
  new `is_anchor_excluded` helper tests the configured globs against
  the anchor canonical AND every `impl_method_canonical` it backs.
  A user-friendly `exclude_targets = ["application::admin::*"]`
  glob now silences the matching anchor finding (e.g.
  `ports::handler::Handler::handle`) when the impl lives in
  `application::admin::*`, instead of requiring a parallel
  ports-path entry. Concrete-pass exclusion is unchanged (already
  matched against the concrete canonical).
- **Stale `line=0` anchor wording** (P3): the v1.2.2 "Added —
  Anchors as target capabilities for Check B/D" entry promised a
  heuristic file path with `line=0` until span info was added —
  contradicting the round-2 P4 fix that already captures the trait
  method's source location. Wording updated to reference the
  round-2 P4 entry that delivered real `MethodLocation` capture.

Round 4 review (Codex 2026-05-04):

- **Trait visibility uses the shared workspace-canonical set** (P1):
  the round-3 trait visibility filter implemented a private
  `node.vis == Public` check (later patched with `enclosing_mod_visible`
  tracking), which diverged from the rest of call-parity's
  visibility model — `pub(crate)`, `pub(super)`, `pub(in <path>)`,
  file-backed module visibility, and `pub use` re-exports were all
  missed. `pub(crate) trait Handler` with a target impl was rejected
  as invisible; conversely a `pub trait` in a private file-backed
  module could still slip through. Fix: `populate_anchor_index` now
  reuses the workspace-wide `visible_canonicals` set built by
  `pub_fns_visibility::collect_visible_type_canonicals_workspace`
  (the same set `pub_fns::collect_pub_fns_by_layer` consumes), so
  trait visibility agrees with the rest of call-parity. Removed the
  redundant `WorkspaceTypeIndex.trait_visibility` map and the
  `TraitCollector.enclosing_mod_visible` tracking — both subsumed
  by the canonical-set lookup.
- **Inherited-default capability gap surfaced** (P2 — superseded
  in round 5, replaced with edge-rewrite in round 7): round 4
  noted that `pub trait Handler { fn handle(&self) {} } impl
  Handler for AppHandler {}` (no override, inherits default body)
  left adapter coverage with no visible target capability. The
  round-4 fix (`callable_impls_for` widening of `impl_layers` /
  `impl_method_canonicals`) was reverted in round 5 because it
  caused canonical collisions with inherent methods of the same
  name and promoted non-target default bodies through empty target
  impls. The final fix lives in the round-7 edge-rewrite pass
  (see below) — see the round 7 review entry.
- **Stale `add_anchor_to_impl_edges` reference** (P3): the
  `trait_dispatch_edges` doc comment in `calls.rs` claimed
  reachability from anchor to impl bodies was wired by
  `workspace_graph::add_anchor_to_impl_edges` — function never
  existed; the design intentionally keeps the anchor as a leaf
  in the graph. Comment updated to reflect the actual behaviour.
- **Default-only target anchor in book summary** (P3): the
  short summary in `book/adapter-parity.md` (the type-inference
  capability list) still said anchors are recognised "when at least
  one overriding impl lives in the target layer". The detailed
  anchor section already documents the default-OR-overriding rule,
  but the summary contradicted it. Updated.

Round 5 review (Codex 2026-05-12):

- **Revert of round-4 `callable_impls_for` widening** (P1 #1 + #2):
  the round-4 expansion of `impl_method_canonicals` to absorb
  non-overriding impls when the trait method has a default body
  caused two distinct bugs. First, `<Impl>::<method>` canonicals
  fabricated for inherited-default impls collide with unrelated
  inherent methods of the same name (`impl X { fn handle … }` +
  `impl T for X {}`), so Check B silently treats the real inherent
  method as anchor-backed and skips it. Second, ports-declared
  default methods with empty target-layer impls falsely promoted
  the anchor to a target capability — the executable body lives on
  the ports trait, not target, so Check A/B/D would require parity
  for code that never crosses into target. Fix: revert to strict
  overriding-only via the restored `overriding_impls_for` accessor;
  inherited-default impls no longer contribute to `impl_layers` or
  `impl_method_canonicals`. Promotion to target capability now
  requires either (a) the trait is declared in the target layer
  with a callable body (default body in target OR an overriding
  impl somewhere), or (b) at least one overriding impl lives in the
  target layer; default bodies declared OUTSIDE target don't promote
  through empty target impls. Unambiguous inherited-default concrete
  calls are folded onto the trait anchor by the round-7 edge-rewrite
  pass, so drift on them is counted against the anchor. The remaining
  blind spot is the ambiguous multi-trait default case (a type
  implementing two traits with the same default method name): the
  round-8 ambiguity guard leaves those phantoms in place rather than
  guessing.
- **Book visibility wording aligned with shared canonical set**
  (P3): the detailed anchor definition in `book/adapter-parity.md`
  still said workspace-visible means "the trait's own `vis` is
  `pub` AND every enclosing inline `mod` is `pub`". The code uses
  the shared `visible_canonicals` set (covering `pub(crate)`,
  `pub(super)`, `pub(in <path>)`, file-backed module visibility,
  and `pub use` re-exports). Wording updated to match.

Round 6 review (Codex 2026-05-12):

- **Walker phantom-canonical gate** (P1): `populate_layer_cache`
  caches `layer_of` for every canonical that appears in the graph,
  including edge sinks. A fabricated `<Impl>::<method>` from an
  inherited-default impl (no override, body lives on the trait)
  therefore got `layer_of == target_layer` and was accepted as a
  target boundary by `is_target_boundary` even though no real fn
  node existed with that canonical. Check A would pass on the
  phantom touchpoint while Check B/D had no way to enumerate the
  same capability consistently. Fix: `is_target_boundary` (in
  `touchpoints.rs`) and the sister `is_target_capability_node` (in
  `check_b_coverage.rs`) now require `graph.forward.contains_key`
  in addition to the layer match for concrete canonicals. Trait
  anchors continue through the unified `is_anchor_target_capability`
  rule untouched. Regression tests
  `touchpoints_reject_phantom_inherited_default_concrete_canonical`
  and `touchpoints_recognise_real_target_fn_node`.
- **Anchor docs round-5 leftover** (P3): the short anchor summary
  in `book/adapter-parity.md`'s type-inference list still said
  "at least one impl in the target layer makes the method callable
  (overriding the signature, or inheriting a default body declared
  elsewhere)" — that was the round-4 widening, reverted in round 5.
  Updated to strict "at least one overriding impl lives in target",
  plus an explicit note that inherited-default impls don't promote.

Round 7 review (Codex 2026-05-12):

- **Phantom inherited-default edge rewrite** (P2): the round-6
  walker-phantom-gate correctly rejected fabricated
  `<Impl>::<method>` canonicals as touchpoints, but never emitted
  an alternative — a target-layer trait declared with a default
  body + empty target impl + adapter UFCS call would silently look
  non-delegating, even though the trait anchor IS a valid target
  capability. New post-build pass
  `workspace_graph::edge_rewrite::rewrite_phantom_inherited_default_edges`
  scans every emitted edge after `FileFnCollector` completes,
  identifies phantom callees that match an inherited-default impl
  (impl is in `trait_impls[T]`, method has default body, impl
  doesn't override), and rewrites the edge to point at the trait
  anchor `<Trait>::<method>`. Concrete inherent methods stay
  untouched (their canonical IS a real graph node), and overriding
  impls stay untouched (their override registers a real fn body).
  Regression test
  `touchpoints_route_inherited_default_concrete_to_anchor`.
- **`call_depth` describes edge depth, not helper hops** (P3): the
  Rustdoc on `CallParityConfig::call_depth`, the
  `book/adapter-parity.md` walk description, the
  `book/reference-configuration.md` table entry, and the
  `docs/internals.md` summary all said "max helper hops", which
  was off-by-one — direct callees are seeded at depth 1, so
  `call_depth = 3` reaches `handler → h1 → h2 → target`
  (three edges, two intermediate helpers). All four sources
  updated with explicit edge-count wording + the example.
- **README stale `mod foo;` limitation** (P3): the "External file
  modules" entry in `README.md`'s Known Limitations claimed
  `mod foo;` declarations weren't followed and only inline modules
  were analysed recursively. That hasn't been true since the
  `file_visibility::collect_file_root_visibility` pre-pass
  shipped with regression tests for crate-root `mod`, private
  file modules, and ancestor chains. Entry removed.

Round 8 review (Codex 2026-05-12):

- **Edge-rewrite ambiguity guard** (P2): the round-7
  `inherited_default_anchor_for` returned the first HashMap match
  when a type implemented multiple traits with the same default
  method name (e.g. `pub trait Greeting { fn handle(&self) {} }`
  and `pub trait Logging { fn handle(&self) {} }` both implemented
  by `AppHandler`). Rewrite choice depended on map iteration order
  — non-deterministic. Rust itself requires UFCS disambiguation
  in that case, and the canonical alone doesn't tell us which
  trait was selected. Fix: rewrite only when EXACTLY ONE
  inherited-default candidate exists; otherwise leave the phantom
  canonical in place (the walker phantom-gate suppresses it).
  Regression test
  `touchpoints_skip_rewrite_when_multiple_traits_share_default_method_name`.
- **CHANGELOG round-4 anchor semantics superseded** (P3): the
  "Inherited-default impls count as target capability" entry from
  round 4 described the `callable_impls_for` widening that was
  reverted in round 5 and properly fixed via edge-rewrite in
  round 7. The CHANGELOG now reads as if two contradictory anchor
  models are active. The round-4 entry is reworded as
  "superseded" with a pointer to the round-7 entry that delivered
  the real fix.
- **CHANGELOG anchor model summary aligned** (P3): the Added-
  section blurb on the round-1 anchor model still framed target
  capability around "overriding impls" and treated inherited
  defaults as sharing the same target semantics. Updated to the
  current dual-rule (target-declared default body OR overriding
  impl in target) plus an explicit note that inherited-default
  UFCS calls are routed via edge-rewrite and that calls inside the
  default-method body itself stay invisible.
- **`inspect_anchor` comment refreshed** (P3): the doc on
  `inspect_anchor` still said "all-direct inherited-default drift
  stays undetected — the impl-method canonical is phantom". With
  the round-7 edge-rewrite, those phantom canonicals are folded
  onto the anchor before coverage/counting, so the limitation no
  longer applies. Comment updated to reflect the active behaviour.

Round 9 review (Codex 2026-05-12, doc-only):

- **Round-5 limitation note narrowed** (P3): the round-5 entry
  describing "mixed-form multiplicity drift on inherited defaults
  remains undetected" was reworded to reflect the round-7
  edge-rewrite — only the ambiguous multi-trait default case (the
  round-8 ambiguity guard) leaves edges phantom now.
- **Top-level anchor summary aligned** (P3): the primary
  `### Added` blurb on the anchor model was framing target boundary
  status around "overriding impl in target" only. Updated to the
  full dual-rule (target-declared callable body OR overriding impl
  in target) plus an explicit note on the edge-rewrite folding for
  unambiguous inherited-default UFCS calls.

Round 10 review (Codex 2026-05-12, doc/comment-only):

- **Ambiguous-multi-trait-default added to known limitations** (P3):
  `book/adapter-parity.md` Limitations list gained a sixth entry
  documenting that UFCS calls like `X::handle(&x)` are left
  unresolved when `X` implements multiple traits with the same
  default method name (Rust requires UFCS disambiguation; the
  canonical alone is ambiguous). Workaround: rename, override on
  the impl, or call through `dyn Trait`.
- **`docs/internals.md` anchor summary refreshed** (P3): the
  contributor-facing summary still framed target-boundary status
  around "at least one overriding impl in target". Rewritten to
  reference `is_anchor_target_capability` directly, list the dual
  rule, mention visibility / peer-adapter constraints, and call out
  the round-7 edge-rewrite for inherited-default UFCS calls.
- **`calls.rs` Rustdoc comments aligned** (P3): the
  `resolve_method_targets` doc still said trait-dispatch inference
  "may return multiple (one per impl of the trait)" — that was
  round-1 behaviour, before the synthetic-anchor collapse. The
  `canonical_edges_for_method` doc had the same "overriding impl
  in target" framing. Both updated to the current single-anchor
  semantics + dual-rule capability predicate.

Round 11 review (Codex 2026-05-12, doc-only):

- **Limitations section heading + intro generalised** (P3): the
  `book/adapter-parity.md` Limitations subsection was titled
  "Limitations: type aliases" with an intro saying "two alias
  patterns currently disagree", but the list had grown to six
  bullets covering re-exports, function re-exports, trait
  default-body internals, and ambiguous inherited-default UFCS
  calls. Renamed to "Known limitations" with an intro that
  classifies each bullet's topic, so readers no longer assume only
  the first two items are in scope.

Round 12 review (Codex 2026-05-12):

- **JSON reporter dropped `logic_count` + `call_count`** (P2): the
  v1.2.1 typed-reporter refactor split `FunctionAnalysis.complexity`
  (legacy IOSP type carrying every metric) into
  `ComplexityMetricsRecord` (typed dimension state), but
  `project_metrics` did not carry the IOSP `logic_count` /
  `call_count` fields across and `json::functions::build_functions`
  hard-coded `JsonComplexity.logic_count` / `call_count` to `0`.
  Every JSON consumer therefore saw zeros for every function since
  v1.2.1, even though the analyzer measured non-zero counts. The
  existing `test_print_json_with_complexity_no_panic` set non-zero
  inputs but only asserted "no panic" — the smoke-test masked the
  data loss for eleven Codex passes. Fix: added the two counts to
  `ComplexityMetricsRecord`, copied them in `project_metrics`, and
  pulled them through `build_functions`. Regression test
  `json_complexity_carries_logic_count_and_call_count` parses the
  produced JSON and asserts the non-zero values survive the
  projection + reporter round-trip.

Round 13 review (Codex 2026-05-12):

- **NearDuplicate similarity dropped from `DryFindingDetails::Duplicate`**
  (P2): the v1.2.1 typed-reporter refactor projected
  `DuplicateKind::NearDuplicate { similarity }` to
  `DryFindingDetails::Duplicate { participants }` and dropped the
  similarity score. JSON output hardcoded `similarity: None` for
  every group, so machine consumers couldn't distinguish a 0.91
  near-duplicate from an unscored exact group. Fix: added
  `similarity: Option<f64>` to the details variant, copied it in
  `project_duplicate_group`, and read it in the JSON builder. `Eq`
  derives dropped on `DryFinding` / `DryFindingDetails`. Regression
  test `test_print_json_carries_near_duplicate_similarity`.
- **RepeatedMatch `arm_count` dropped and groups collapsed by
  enum name** (P2): the typed `RepeatedMatchParticipant` carried
  no `arm_count`, so JSON `entries[].arm_count` was hardcoded to
  `0`; the JSON builder additionally de-duplicated groups by
  `enum_name` alone, collapsing two distinct repeated patterns over
  the same enum into one group. Fix: added `arm_count: usize` to
  `RepeatedMatchParticipant`, copied it in projection, and read it
  in the JSON builder. JSON de-dup keyed by
  `(enum_name, sorted participant locations)`. Regression test
  `test_print_json_carries_repeated_match_arm_count_and_distinct_groups`.
- **SRP `composite_score`, responsibility clusters, and
  `length_score` dropped** (P2): `SrpFindingDetails::StructCohesion`
  was missing the analyzer's `composite_score` + `clusters`
  (responsibility groups), and `ModuleLength` was missing
  `length_score`; JSON consumers filled placeholder zeros / empty
  arrays / wrapped-one-element-arrays. Fix: added the missing
  fields plus a new `ResponsibilityCluster` domain type
  (re-exported from `domain::findings`), copied them in
  `project_struct` / `project_module`, and pulled them through the
  JSON builder. The intermediate `SrpModuleRow` (text/html-friendly)
  still flattens each cluster's member list with `", "`; JSON
  preserves the per-cluster grouping. `Eq` derives dropped on
  `SrpFinding` / `SrpFindingDetails` (the new `f64` fields are not
  `Eq`). Regression test
  `test_print_json_carries_srp_composite_score_clusters_length_score`.
- **Integration test renamed and reinforced** (proactive A21
  sweep): `test_json_output_parseable` was a schema-only smoke
  test. Renamed to `test_json_output_schema_and_complexity_values`
  and extended with a value assertion that at least one
  `functions[].complexity.logic_count` is non-zero on
  `examples/sample.rs`. Catches future projection drops in the
  JSON path because `sample.rs` deterministically has Operations
  with non-zero logic counts.

Round 14 review (user-driven proactive A21 sweep, 2026-05-12):

- **All 24 reporter `_no_panic` smoke tests converted to
  value-asserting tests** (proactive A21-class elimination): rounds
  12-13 surfaced three v1.2.1 typed-reporter refactor drops that
  smoke tests had masked (JSON `logic_count`/`call_count`,
  NearDuplicate `similarity`, RepeatedMatch `arm_count`, SRP
  `composite_score`/`clusters`/`length_score`). Rather than wait
  for Codex to discover the remaining smoke tests one round at a
  time, the user directed a full sweep. All 24 `_no_panic` tests
  across eight reporters (sarif=3, ai=3, dot=3, findings_list=2,
  github=2, json=4 remaining, text=6, pipeline=1) were replaced
  with tests that assert actual output values and renamed to
  describe the asserted behavior (e.g.
  `test_print_json_carries_violation_logic_and_call_locations`,
  `test_print_sarif_emits_violation_with_location`,
  `ai_value_includes_complexity_finding_metric_and_location`). New
  helper `format_findings(&[FindingEntry]) -> String` in
  `findings_list/mod.rs` (string-returning variant of
  `print_findings`) makes the findings-list reporter testable
  without stdout capture. Reporter test fixtures now populate
  `findings.iosp` via `project_iosp` so the projection path is
  actually exercised. Test count unchanged (1611 → 1611, 1-for-1
  replacement). No production code changes — the conversion is
  purely a test-suite hardening to prevent future projection
  drops from staying silent. `grep -rn "fn .*_no_panic\|fn
  .*_no_crash" src/ tests/` returns nothing after this sweep, so
  the smoke-test category is effectively eliminated from the
  reporter suite.

Round 15 review (Codex 2026-05-13):

- **Repeated-match dedup leaked into text/HTML via shared
  projection** (P2): round 13's fix routed the JSON repeated-match
  builder to a `(enum_name, sorted participant locations)` dedup
  key, but the shared `split_dry_findings` projection
  (`src/adapters/report/projections/dry.rs`) — consumed by the
  text and HTML reporters — still deduped by `enum_name` alone.
  Two distinct repeated-match patterns over the same enum
  therefore collapsed into one rendered group outside JSON, so
  reporter parity regressed in the very next pass. Fix:
  `build_repeated_match_groups` now goes through the existing
  `dedup_by_locations` helper (same path that `build_duplicate_groups`
  and `build_fragment_groups` use), keying on the participant
  location set. Regression tests
  `split_dry_findings_keeps_distinct_repeated_match_groups_over_same_enum`
  + `split_dry_findings_collapses_duplicate_repeated_match_group_emissions`
  in `src/adapters/report/tests/projections_dry.rs` lock the dedup
  contract at the projection layer so every reporter benefits.

Round 16 review (Codex 2026-05-13):

- **Aliased `Future` bound on `impl Trait` lost its `Output`**
  (P2): `resolve_bound_list` in
  `src/adapters/analyzers/architecture/call_parity_rule/type_infer/resolve.rs`
  checked the raw bound leaf with `last.ident == "Future"` before
  alias canonicalisation. With
  `use std::future::Future as Fut; fn make() -> impl Fut<Output = Session>`,
  the leaf was `Fut` so the Future-detection branch missed, the
  bound got recorded as `TraitBound(std::future::Future)` instead,
  and `make().await.diff()` stayed unresolved because the canonical
  type no longer exposed the `Output = Session` shape. Fix: routed
  the bound through `identify_wrapper_name` (the same alias-aware
  probe `resolve_path` uses for path-form `Future<Output = T>`),
  keeping the original `Output = T` args from the trait bound so
  `wrap_future_output` can resolve them. Regression test
  `test_impl_aliased_future_resolves_to_future_with_output`
  asserts `Future(Session)` for the aliased form.
- **Check-A diagnostic still said "hops"** (P3): round 11 renamed
  `call_depth` to call-edge depth in the config doc + book to
  remove the off-by-one ambiguity (`3` = three call edges, two
  intermediate helpers — not three nodes). The emitted Check-A
  message in `rendering.rs` still said
  "within {call_depth} hops", keeping the ambiguity alive in real
  user-facing diagnostics. Reworded to
  "within {call_depth} adapter-internal call edges". The example
  in `book/adapter-parity.md:194` was synced. Regression test
  `no_delegation_message_uses_call_edge_wording_not_hops` locks
  the new wording.

Round 17 review (Codex 2026-05-13):

- **Marker-trait skip discarded workspace traits with marker-style
  leaf names** (P2): `resolve_bound_list` skipped each bound via
  `is_marker_trait`, which checked the raw last segment against a
  hard-coded `MARKER_TRAITS` list before alias canonicalisation.
  Workspace traits or aliases like `dyn crate::ports::Send` or
  `use crate::ports::Handler as Send; dyn Send` therefore got
  discarded as if they were the std marker, so `h.handle()` never
  became a trait anchor. Same root cause as round 16 P2 (aliased
  `Future` bound) — a sister-fix-site that should have been caught
  in the same pass. Fix: extracted `is_marker_trait` into a new
  `resolve_marker` module that canonicalises the bound first and
  skips only when the canonical leaf is in `MARKER_TRAITS` AND the
  canonical path is stdlib-prefixed (`std`/`core`/`alloc`).
  Unresolvable paths still skip bare single-segment markers
  (`dyn Send` via prelude) and explicitly stdlib-rooted forms
  (`dyn std::marker::Send`) — multi-segment workspace paths that
  failed to canonicalise are treated as real bounds. Regression
  tests `test_impl_trait_local_send_named_trait_resolves_not_skipped`
  + `test_impl_trait_bare_std_send_marker_still_skipped` cover both
  directions.

Round 18 review (Codex 2026-05-13):

- **External aliased trait bounds shadowed later workspace
  bounds** (P2): after the round-17 marker fix, the bound resolver
  in `resolve_bound_list` still accepted any successfully-canonicalised
  path as a `TraitBound`. With `use serde::Serialize;` and
  `fn make() -> impl Serialize + Handler`, the first bound expanded
  to `["serde", "Serialize"]` and returned, so the later workspace
  `Handler` bound was never visited — `make().handle()` stayed
  unresolved. Fully-qualified `serde::Serialize` without the `use`
  alias already returned `None` from canonicalisation and was
  correctly skipped; the alias-expanded form took a different path
  and slipped through. Fix: gate the `TraitBound` return on
  `canonical.first() == Some("crate")` so only workspace-rooted
  bounds win — external aliases now skip exactly like the
  fully-qualified external form. The std-marker special case
  (`resolve_marker::is_marker_trait`) and the Future special case
  (`future_bound_args`) still run first, so `Send` / `Sync` /
  `Future<Output = T>` keep their existing handling. Regression
  test `test_impl_trait_external_aliased_bound_skipped_workspace_bound_wins`.

### Added

- **Trait-method anchor model for call-parity dispatch**: `dyn
  Trait.method()` now emits a single synthetic
  `<Trait>::<method>` anchor instead of one edge per overriding
  workspace impl. The boundary walker recognises the anchor as a
  target boundary when (a) the trait is declared in the target
  layer with a callable body (default body in target OR an
  overriding impl), or (b) at least one overriding impl lives in
  the target layer; non-target default bodies are not promoted
  through empty target impls (`CallGraph::trait_method_anchors`
  populated by `populate_anchor_index`). Concrete impl-method
  canonicals never enter the touchpoint set via dispatch, so
  Check C doesn't fire on what is semantically a single boundary
  call. Unambiguous inherited-default UFCS calls are routed to
  the same anchor by the edge-rewrite post-pass.
- **Anchors as target capabilities for Check B/D**:
  `CallGraph::target_anchor_capabilities(target)` enumerates
  trait-method anchors that pass the unified target-capability
  rule (target-declared callable body OR overriding impl in
  target). Check B iterates them alongside concrete
  `pub_fns_by_layer[target]`, so dispatch-only adapter coverage
  is checked for parity and orphan status; Check D counts handlers
  per anchor for multiplicity. Anchor findings carry the trait
  method's actual source location (file + 1-based line + column)
  — see the round-2 P4 entry above for the `MethodLocation`
  capture path.
- **Walker peer-adapter check before anchor promotion**:
  `TouchpointWalk::run` now checks `is_peer_adapter` BEFORE
  `is_target_boundary`. A trait anchor declared in a peer-adapter
  layer (e.g. `mcp::Handler`) with overriding impls in the target
  layer no longer leaks peer-adapter coverage into the origin
  adapter's set.
- **`OrphanSuppression` Finding type in `domain::findings`** with
  `AnalysisFindings::orphan_suppressions` field. Cross-cutting
  Finding (not tied to a single dimension) carrying
  `// qual:allow(...)` markers that matched no finding in their
  annotation window.
- **`ReporterImpl::OrphanView` + `build_orphans` method** plus
  `Snapshot::orphans` field. Per-reporter discretion: dot stays
  `OrphanView = ()` (intentional no-op for the data-only graph
  format), the seven diagnostic reporters (text, html, json, sarif,
  github, ai, findings_list) declare meaningful view types and
  consume `snapshot.orphans` exclusively. Future reporters MUST
  implement `build_orphans` (compile-force) and consciously decide
  what to do with orphans.

### Fixed

- **cfg-test impl-block leak in graph + pub-fn visitors**:
  `file_fn_collector::visit_item_impl` and `pub_fns::visit_item_impl`
  now skip `#[cfg(test)] impl X { … }` blocks entirely. Previously
  the cfg attribute lived on the impl block while child methods had
  no attrs of their own, so test-only methods leaked into the
  production call graph and pub-fn surface.
- **`record_trait_impl` filters cfg-test / `#[test]` overrides**:
  `WorkspaceTypeIndex.trait_impl_overrides` no longer records
  test-only methods, so production dispatch can't route to a phantom
  `Type::method` for a test-only override.
- **Strict self-type visibility in pub_fns**: trait-impl method
  registration was relaxed in an interim fix to register
  `<Hidden>::<method>` for `impl PubTrait for Hidden` even when
  `Hidden` is private. With the anchor refactor that relaxation is
  no longer needed and produced over-coverage; the strict visibility
  gate is restored. The trait method's anchor carries the public
  capability instead.

### Removed

- **`AnalysisResult.orphan_suppressions` field** — orphan rendering
  flows exclusively through `findings.orphan_suppressions`
  (consumed by `Snapshot::orphans` per reporter). The legacy
  struct-field bypass and per-reporter `orphan_suppressions: &'a [_]`
  fields are gone.
- **`OrphanSuppressionWarning`** alias removed. The canonical type is
  `domain::findings::OrphanSuppression`; the adapter-layer alias served
  as a transition step and is no longer needed.

## [1.2.2] - 2026-04-30

Patch release: **Reporter-Trait sealed two-trait + Snapshot pattern**.

Internal refactor — no user-visible behaviour change. Every output
format (text, html, json, sarif, github, ai, dot, findings_list) now
goes through a single `Reporter::render()` entry point backed by a
sealed `ReporterImpl` trait. The compile-time Reporter-Parity guarantee
(adding a new dimension forces every reporter to address it) is now
proven by three orthogonal failure modes simultaneously: trait method
set, snapshot constructor, and exhaustive `publish` destructuring —
verified in Phase 11 by introducing a synthetic 8th dimension and
observing 18 compile errors across all 9 reporter sites.

### Changed

- **Sealed two-trait design** in `src/ports/reporter.rs`: public
  `Reporter` trait with single `render()` method (only entry point
  external code can invoke), crate-internal `ReporterImpl` with
  per-dim `build_*` projections and `publish()` composition. The
  `sealed::Sealed` supertrait lives in a private module so no external
  crate can implement `Reporter` directly. `Snapshot<R>` aggregates
  all 10 per-dim views with `pub(crate)` fields, locking
  `ReporterImpl::publish` to crate-internal callers.
- **Per-reporter pure-data Views**: every reporter projects findings
  into typed row structs (`HtmlIospView`, `SarifResultRow`,
  `AiIospRow`, etc.); `publish()` formats them into the final string.
  No reporter pre-renders markup in `build_*` anymore — composition
  decisions (card-then-table-then-cross-section in HTML, summary-then-
  details in text, etc.) live in `publish()`.
- **Cross-reporter shared projections** in
  `src/adapters/report/projections/{srp, coupling, dry, tq}.rs`:
  text/html/sarif/json/ai/findings_list reporters all consume the
  same dimension-bucket projections (`SrpBuckets`, `CouplingBuckets`,
  `DryBuckets`, etc.). Removed twelve transitional cross-reporter
  duplicate findings via these helpers.
- **Pipeline.rs** (`src/app/pipeline.rs`): every output-format branch
  follows the unified `<Reporter>.render(&findings, &data)` shape.
  Print wrappers stay as the boundary entry points.

### Removed

- Legacy `DeprecatedReporter` + `DeprecatedAnalysisReporter` traits
  and the `deprecated_render_findings` / `deprecated_render_analysis_data`
  helpers — fully replaced by the sealed design.

### Fixed

- **Trait-dispatch collapses to synthetic anchor.**
  `calls::trait_dispatch_edges` previously emitted `<impl>::<method>`
  for every workspace impl of a dispatched trait method. A single
  `h.handle()` on `dyn Handler` with N overriding impls produced N
  edges, expanding into N touchpoints in the boundary walker, which
  triggered Check C `multi_touchpoint` warnings for what is
  semantically a single boundary call. Dispatch now emits ONE
  synthetic anchor `<Trait>::<method>` representing the logical
  capability. The touchpoint walker recognises the anchor as a
  target boundary when (a) the trait is declared in the target
  layer with a callable body (default OR overriding impl), OR
  (b) at least one overriding impl lives in the target layer —
  non-target default bodies are NOT promoted through empty target
  impls (the executable body lives outside target). Concrete UFCS
  calls into inherited-default impls are routed to the anchor at
  graph build time via the edge-rewrite post-pass, so dispatch and
  direct-concrete forms share the same anchor. Calls **inside**
  the default-method body itself stay invisible to Check A/B/D
  (the trait method's body isn't a graph node).
- **`record_trait_impl` filters cfg-test / `#[test]` overrides.**
  `WorkspaceTypeIndex.trait_impl_overrides` used to record every
  `ImplItem::Fn`, including test-only methods. Production dispatch
  then routed to a phantom `Type::method` for a test-only override
  while the workspace call graph + `method_returns` index correctly
  skipped those items. The override set is now filtered with
  `has_cfg_test` + `has_test_attr`, mirroring `methods.rs`.
- **PubFnCollector keeps strict self-type visibility.** Earlier
  v1.2.2 relaxed visibility to register `<Hidden>::<method>` for
  `impl PubTrait for Hidden` even when `Hidden` is private, so
  dispatch-emitted impl-edges had matching pub-fn entries. With
  the anchor refactor dispatch no longer emits per-impl edges, so
  the relaxation is unnecessary and the visibility gate is
  restored to its strict form: only impls on visible self-types
  contribute concrete target pub-fns. Private impls are still
  reachable through the anchor — the public capability they
  fulfill — without polluting the per-handler-type pub-fn surface.
  Regression test `test_collect_pub_fns_skips_trait_impl_method_on_private_self_type`.

### Documented limitations

- Function re-exports (`pub use private::op` for `pub fn op()`) are
  intentionally filtered from the visible-types set so private
  same-named types don't leak. The trade-off — pub-use-only
  functions are blind to Check B/D — is documented as Limitation #4
  in `book/adapter-parity.md`. Workaround: declare the function at
  a publicly-reachable path directly.

### Internal

- 1565 tests, 100% quality across all seven dimensions, 0 findings,
  0 clippy warnings.
- All `qual:allow(dry)` and `qual:allow(srp)` markers added during
  the migration phases removed: github helpers refactored to a
  generic `GithubDetailRow<D>` + `build_detail_view` /
  `format_detail_view`, html dry tables share a generic
  `render_table<T>`, sarif `SarifResultRow` holds the whole `Finding`
  (single clone) instead of destructured fields, html coupling
  introduces a private `format_subsections` helper to merge the three
  sub-formatters into one cluster, and ai is split into
  `ai/{mod, rows, format, details, output}.rs`.
- New regression test `helper_reached_via_trait_blanket_dispatch_is_not_dead_code`
  in `src/adapters/analyzers/dry/tests/dead_code.rs` documents that
  the `call_targets` visitor handles the trait-blanket-dispatch case
  via flat method-name capture; the v1.2.2 `sarif_rules` workaround
  was unnecessary and has been reverted.

## [1.2.1] - 2026-04-27

Patch release: **`call_parity` boundary semantic + new Checks C/D**.

The v1.2.0 `call_parity` rule walked transitive reachability across
the entire target layer up to `call_depth` hops. On a clean codebase
with zero genuine adapter asymmetries, this still produced findings
for every application-internal helper that wasn't directly touched
by every adapter (e.g. `record_operation`, `impact_count`). The
findings pointed *inward* at application plumbing rather than at
real adapter drift.

v1.2.1 reframes Check B's semantic to **boundary-only**: walk forward
from each adapter pub-fn until the target layer is hit, record that
node as the adapter's touchpoint, then stop. Compare touchpoint sets
across adapters. Application-internal helpers are no longer inspected
for parity — that's `DRY-002`'s concern, not `call_parity`'s.

### Added

- **Check C — multi-touchpoint** (`architecture/call_parity/multi_touchpoint`):
  flags adapter pub-fns that orchestrate across multiple application
  calls themselves. Configurable severity via
  `[architecture.call_parity] single_touchpoint = "off" | "warn" | "error"`,
  default `"warn"` (emits as `Severity::Low`).
- **Check D — multiplicity mismatch**
  (`architecture/call_parity/multiplicity_mismatch`): flags target
  pub-fns reached by every adapter but with divergent per-adapter
  handler counts (e.g. cli has 2 handlers → `session.search`, mcp
  has 1).
- **Deprecated-handler exclusion**: adapter pub-fns marked
  `#[deprecated]` (in any form) are excluded from Checks A/B/C/D.
  Aliases that are explicitly being phased out shouldn't drag the
  parity report.
- Regression tests pinning correct turbofish + inferred-generic call
  resolution behavior in the canonical-call collector.

### Changed

- **Check B — boundary semantic**. A target pub-fn is flagged when:
  - it appears in some adapter's coverage but is missing from another
    (mismatch case — adapter feature drift), OR
  - it isn't transitively reachable from any adapter touchpoint
    through target-internal callers (orphan case — application
    capability not wired to any adapter, including dead target-layer
    islands where only other unreachable target fns call it).
  Internal application chains wired up via at least one adapter
  (`session.search → record_operation → impact_count` when an adapter
  reaches `session.search`) are silent.
- `call_depth` semantic narrowed: now bounds **adapter-internal**
  traversal depth only. Once the target layer is reached, the walk
  stops descending into target callees. Default unchanged (3); no
  config breakage.

### Migration notes

If you saw v1.2.0 fire findings on application-internal helpers
(`record_operation`, `impact_count`, etc.) that ARE wired up through
some adapter, those silently disappear under v1.2.1. The legitimate
adapter-asymmetry findings remain. Genuinely orphaned target pub-fns
— including those only callable via other dead target-layer code —
still produce findings under Check B's orphan branch.

If you want to detect "internal application helpers reached
asymmetrically through other application code", that semantic is no
longer covered by `call_parity`; use `DRY-002` (dead code) plus the
existing per-target visibility audit in code review.

### Architecture refactor: typed per-dimension Findings

Alongside the call_parity bugfix, v1.2.1 introduces a **typed
per-dimension Finding architecture** that fixes a long-standing
"shotgun surgery" pattern: when a new dimension was added, every
reporter had to be touched manually and gaps went unnoticed (e.g.
`architecture_findings` only appeared in JSON/SARIF/findings_list,
silently missing from HTML/AI/text/github).

#### Added

- `domain::findings::*` — seven typed Finding structs (`IospFinding`,
  `ComplexityFinding`, `DryFinding`, `SrpFinding`, `CouplingFinding`,
  `TqFinding`, `ArchitectureFinding`) plus `AnalysisFindings`
  aggregate. Each typed Finding embeds `domain::Finding` as `common`
  for shared metadata (file/line/column/dimension/rule_id/message/
  severity/suppressed) and adds dimension-specific detail.
- `domain::analysis_data::*` — typed state structures (`FunctionRecord`,
  `ModuleCouplingRecord`) that carry per-function classification +
  complexity metrics and per-module coupling metrics for reporters.
- `ports::reporter::Reporter` trait with one method per dimension
  (no default implementations). The compile-time guarantee: when a new
  dimension is added, every reporter that hasn't been migrated fails
  to compile. `render_report` helper visits all dimensions in
  canonical order.
- `app::projection` module with per-dimension projection adapters that
  build typed Findings + AnalysisData from the analyzer outputs.
  Pipeline populates `AnalysisResult.findings` and
  `AnalysisResult.data` directly.
- Architecture findings now visible in **all reporters** (HTML, AI,
  JSON, SARIF, findings_list, text-verbose, github). Previously
  rendered only by JSON/SARIF/findings_list.
- AI reporter: `map_category("ARCHITECTURE") → "architecture"`
  (previously fell through unmapped).
- Per-kind metadata helpers consolidate label lookups: `DryFindingKind::meta()`,
  `TqFindingKind::meta()`, `ComplexityFindingKind::meta()`,
  `Severity::levels()` — replaces the kind→string match statements
  that used to be duplicated across reporters.

#### Changed

- `AnalysisResult` reduced to 5 fields: `results` (FunctionAnalysis
  records), `summary`, `orphan_suppressions`, `findings` (typed
  per-dimension), `data` (typed per-dimension state). The legacy
  per-dimension fields (`coupling`, `duplicates`, `dead_code`,
  `fragments`, `boilerplate`, `wildcard_warnings`, `repeated_matches`,
  `srp`, `tq`, `structural`, `architecture_findings`) are removed —
  every reporter now consumes the typed findings/data exclusively.

#### Migration notes

For consumers of the JSON output: no breaking changes — JSON shape is
unchanged. The typed `findings` and `data` aggregates are the internal
input the pipeline projects from; the JSON envelope is built from
them with the same shape as before.

For maintainers: when adding a new dimension, the migration path is
now (1) define the typed `*Finding` struct in `domain::findings`,
(2) add the projection adapter in `app::projection`, (3) extend the
`Reporter` trait with `report_<new_dim>`, (4) every reporter
implementing the trait fails to compile until updated. This replaces
the old practice of grepping for "where do reporters consume this
dimension?" and hoping nothing was missed.

## [1.2.0] - 2026-04-24

Minor release: **shallow type-inference** for `call_parity` receiver
resolution across three dimensions:

1. **Return-type propagation** (method chains, field access, stdlib
   Result/Option/Future combinators, destructuring patterns) —
   eliminates the dominant false-positive class that made v1.1.0
   unusable on any Session/Context/Handle-pattern Rust codebase.
2. **Trait dispatch over-approximation** — `dyn Trait` / `&dyn Trait` /
   `Box<dyn Trait>` receivers fan out to every workspace impl of the
   trait. Makes the tool structurally sound for Ports&Adapters
   architectures, where dependency inversion via trait objects is the
   core abstraction.
3. **Framework & type-alias config** — type-alias expansion,
   user-configurable transparent wrapper types (Axum `State<T>`,
   Actix `Data<T>`, tower `Router<T>`, …), and attribute-macro
   transparency (with a default starter-pack for `tracing::instrument`,
   `async_trait`, `tokio::main`/`test`, etc.).

No breaking changes; existing `[architecture.call_parity]` configs
keep working without modification — the new resolution paths are all
additive and the legacy fast-path stays intact as a safety net.

### Fixed
- **`call_parity` method-chain constructor resolution.** v1.1.0's
  resolver only extracted binding types from direct constructor calls
  (`let s = T::ctor()`). Real-world Rust code more often wraps the
  constructor in a `?` / `.unwrap()` / `.map_err(…)?` chain, which
  returned `None` from the legacy extractor and left the downstream
  method call as a layer-unknown `<method>:name`. On rlm (the reference
  adopter codebase), this produced 93 of 116 false-positive findings —
  roughly 80 % of the total. Symptom: every CLI handler shaped like
  ```rust
  pub fn cmd_diff(path: &str) -> Result<(), Error> {
      let session = RlmSession::open_cwd().map_err(map_err)?;
      session.diff(path).map_err(map_err)?;
      Ok(())
  }
  ```
  was reported as "not delegating to application" even though it
  obviously did.
- **`self` receiver resolution inside impl methods.** Signature seeding
  only iterated typed `FnArg::Typed` params, never the `self` receiver.
  As a result `self.helper()` and `self.field.method()` fell through to
  `<method>:…` even when the enclosing impl's canonical type was known
  via `self_type`. The collector now binds `self` to the impl's
  canonical segments alongside the typed params, so ordinary
  method-internal delegation routes through `method_returns` /
  `struct_fields` like any other receiver.

### Added
- **`call_parity_rule::type_infer`** — new module implementing shallow
  type inference over `syn::Expr`. Exposes `infer_type(expr, ctx) ->
  Option<CanonicalType>` as the public entry point. Built on three
  layers:
  - `workspace_index`: single pre-pass over the workspace collecting
    struct-field types, impl-method return types, and free-fn return
    types into a lookup index. Runs once per `build_call_graph` call.
  - `infer`: dispatch over expression variants — `Path`, `Call`,
    `MethodCall`, `Field`, `Try` (`?`), `Await`, `Cast`, `Unary(Deref)`,
    plus transparent `Paren` / `Reference` / `Group`. Supports
    `Self::xxx` substitution in impl-method contexts.
  - `combinators`: stdlib table covering `Result<T,E>` / `Option<T>` /
    `Future<T>` — `unwrap`, `expect`, `unwrap_or*`, `ok`, `err`,
    `map_err`, `or_else`, `ok_or`, `filter`, `as_ref` etc. Closure-
    dependent methods (`map`, `and_then`, `then`) intentionally stay
    unresolved rather than fabricate an edge.
- **Pattern-binding walker** (`type_infer::patterns`) — extracts
  `(name, type)` pairs from `let` / `if let` / `while let` / `let …
  else` / `match`-arm / `for` patterns. Handles tuple-struct
  destructuring (`Some(x)`, `Ok(x)`, `Err(_)`), named-field struct
  patterns (`Ctx { session }`, `Ctx { session: s }`, `Ctx { a, .. }`),
  slice patterns with rest, and disambiguates `None` as a variant
  against `Option<_>` instead of binding it as a variable name.
- **Fallback wiring in `calls::CanonicalCallCollector`** — both
  `visit_local` (for binding extraction) and `visit_expr_method_call`
  (for method resolution) now invoke `type_infer` as a fallback after
  the legacy fast-path fails. The fast path (direct constructor
  extraction, signature-parameter types, explicit `let x: T = …`
  annotation) is preserved for unit-test fixtures that don't build a
  workspace index, so no existing tests regressed.
- **`BindingLookup` trait** bridges the legacy `Vec<String>` scope
  stack into the inference engine's `CanonicalType` vocabulary via
  the `CollectorBindings` adapter. Returns owned `Option<CanonicalType>`
  so adapters can synthesize types on the fly without lifetime
  gymnastics.

### Changed
- **`FnContext` in `call_parity_rule::calls`** gained a new
  `workspace_index: Option<&'a WorkspaceTypeIndex>` field. The full
  `build_call_graph` pipeline always passes `Some(&index)`; unit-test
  fixtures pass `None` and fall back to the legacy fast-path only.
  Additive change — no public-API break for existing
  `collect_canonical_calls` call sites.
- **`build_call_graph`** now pre-builds the workspace type-index once
  before the per-file walk. The index shares the same `cfg_test_files`
  filter as the call-graph itself, so the two stay consistent.
- **`iosp::analyze_file`** — bugfix discovered during Task 1.3:
  `file_in_test` was propagated only to free-fn analysis, not to
  `Item::Impl` / `Item::Trait` / `Item::Mod`. This meant any impl-method
  helper inside a `#[cfg(test)] mod tests;` file incorrectly had
  `is_test = false` and got flagged by ERROR_HANDLING / MAGIC_NUMBER /
  LONG_FN checks. Now matches `analyze_mod`'s already-correct
  propagation.

### Documentation
- **`docs/rustqual-design-receiver-type-inference.md`** — the
  normative spec for the multi-stage receiver-resolution work
  (v1.2.0 → v1.3.0 → v1.4.0). Contains the type-inference grammar
  (§3), full stdlib-combinator table (§4), pattern-binding catalog
  (§5), workspace-index schema (§6), trait-dispatch plan (§7),
  config-schema additions (§8), documented Stage-1 limits (§9), and
  test-matrix (§10). Every PR modifying `type_infer/` is reviewed
  against this doc.

### Added — Trait-Dispatch (Stage 2)
- **`dyn Trait` / `&dyn Trait` / `Box<dyn Trait>` receivers** fan out
  to every workspace impl. `fn dispatch(h: &dyn Handler) { h.handle() }`
  records one edge per `impl Handler for X` — sound over-approximation
  that makes call-parity structurally correct for Ports&Adapters
  architectures. Marker traits (`Send`, `Sync`, `Unpin`, `Copy`,
  `Clone`, `Sized`, `Debug`, `Display`) are skipped when picking the
  dispatch-relevant bound from `dyn T1 + T2`.
- **Trait-method gate**: dispatch only fires when the method is in the
  trait's declared method set. `dyn Handler.unrelated_method()` still
  falls through to `<method>:name` rather than fabricating edges.
- **`trait_impls` + `trait_methods` index** built once per
  `build_call_graph`. `impls_of_trait(trait)` and
  `trait_has_method(trait, method)` are the public query methods.
- **Turbofish-as-return-type**: `get::<Session>()` where `get` is a
  generic fn with no concrete workspace return infers `Session` from
  the turbofish arg. Narrow by design — only single-ident paths
  trigger, so `Vec::<u32>::new()` (turbofish on type segment) isn't
  over-approximated.

### Added — Framework & Config Layer (Stage 3)
- **Type-alias expansion.** `type Repo = Arc<Box<Store>>;` recorded in
  the workspace index; `fn h(r: Repo) { r.insert(..) }` expands `Repo`
  → `Arc<Box<Store>>` → `Store` (Arc/Box are Deref-transparent) and
  resolves `insert` against Store's method index. Aliases wrapping
  non-Deref types like `RwLock` / `Mutex` / `RefCell` / `Cell` still
  expand the alias itself, but those wrappers aren't peeled by default
  (their `read` / `lock` / `borrow` methods don't live on the inner
  type) — list them in `transparent_wrappers` if your codebase genuinely
  treats them as Deref-transparent.
- **User-configurable transparent wrappers** via
  `[architecture.call_parity]::transparent_wrappers`:
  ```toml
  [architecture.call_parity]
  transparent_wrappers = ["State", "Extension", "Json", "Data"]
  ```
  Peeled identically to `Arc`/`Box` during resolution. Unblocks
  Axum/Actix-style framework-extractor patterns where
  `fn h(State(db): State<Db>) { db.query() }` would otherwise stay
  unresolved.
- **Attribute-macro transparency** via
  `[architecture.call_parity]::transparent_macros` with a starter-pack
  (`instrument`, `async_trait`, `main`, `test`, `rstest`, `test_case`,
  `pyfunction`, `pymethods`, `wasm_bindgen`, `cfg_attr`) applied by
  default. Current effect is config-schema groundwork + authorial
  intent — the syn-based AST walk already treats attribute macros as
  transparent, so listed entries compile but don't change today's
  behaviour. Retained for future macro-expansion integrations that
  can consult the list without a config-schema break.

### Known Limits
Patterns that intentionally stay unresolved and produce `<method>:name`
fallback markers rather than fabricate edges:
- `Session::open().map(|r| r.m())` — closure-body argument type is
  unknown. Inner method call stays `<method>:m`.
- `fn get<T>() -> T { … }; let x = get(); x.m()` without annotation
  or turbofish. Use `let x: T = get();` or `get::<T>()`.
- `fn make() -> impl Trait { … }; make().inherent_method()` —
  `impl Trait` hides the concrete type by design. Methods declared on
  the trait resolve via trait-dispatch over-approximation; inherent
  methods stay `<method>:name`.
- `fn make() -> impl Future<Output = T> + Handler { … }` — multi-bound
  intersection returns. `CanonicalType` carries one type per receiver,
  so `resolve_bound_list` keeps the first non-marker bound only;
  `.await` propagation *or* trait-dispatch fires, never both. Marker
  traits (`Send` / `Sync` / `Unpin` / `Copy` / `Clone` / `Sized` /
  `Debug` / `Display`) are filtered first, so the common
  `impl Future<Output = T> + Send` shape is unaffected.
- `pub mod outer { … pub use self::private::Hidden; }` followed by
  `fn h(x: outer::Hidden) { x.op() }` — the receiver-type resolver
  doesn't follow workspace-wide `pub use` re-exports inside nested
  modules, so the parameter resolves to `crate::…::outer::Hidden`
  while methods on the impl (inside `mod private`) are indexed under
  `crate::…::outer::private::Hidden`. Visibility recognises both
  paths, but the call-graph edge collapses to `<method>:op`.
  Workaround: write the impl at the file-level qualified path
  (`impl outer::Hidden { … }`) so impl-canonical and caller-canonical
  agree, or `qual:allow(architecture)` at the call-site.
- `pub type Public = private::Hidden; impl Public { pub fn op() }` —
  the impl method is indexed under `crate::…::Public::op` (impl
  self-type via path canonicaliser), but a caller `fn h(x: Public)
  { x.op() }` resolves `x` via type-alias expansion to
  `crate::…::private::Hidden` and emits a `Hidden::op` edge.
  Visibility sees `Public`, but the edges disagree so Check B
  flags `Public::op` as unreached. Workaround: write `impl
  private::Hidden { … }` directly so impl-canonical and
  caller-canonical agree, or `qual:allow(architecture)` on the
  affected impl.
- `type Id<T> = T; pub type Public = Id<private::Hidden>;` — the
  visibility pass doesn't substitute use-site generic args into
  alias bodies (the workspace alias-index runs after pub-fn
  enumeration). `Id` enters `visible_canonicals`, but
  `private::Hidden` doesn't, so Check B can drop public methods on
  `Hidden`. Receiver-side resolution does substitute, so callers
  still reach `Hidden::op`. Workaround: skip the generic-alias
  indirection (`pub type Public = private::Hidden;`), or
  `qual:allow(architecture)` on the affected impl.
- Arbitrary proc-macros that alter the call graph without being in
  `transparent_macros` config. User-annotate via
  `// qual:allow(architecture)` on the enclosing fn.

### Infrastructure
- **`tests/end_to_end_snapshot.rs`** — end-to-end regression snapshot
  with a 3-file session/handler fixture (application/session,
  cli/handlers, mcp/handlers). Asserts a budget of **0 Check A
  findings + 5 Check B findings** (the 5 legitimate asymmetries /
  dead-code items). Any drift in this count is a clear regression
  signal. (Renamed from `tests/rlm_snapshot.rs` in v1.2.4.)
- **`tests/regressions.rs`** — unit-level tests covering every
  method-chain-constructor and cascading-struct-field-access pattern,
  plus Stage-2 trait-dispatch / turbofish cases and Stage-3
  type-alias / user-wrapper cases. Negative tests pin documented
  limits in place.
- **~160 new unit tests** across `type_infer/tests/` covering
  `CanonicalType`, `resolve_type`, workspace-index building, inference
  dispatch, pattern binding, the stdlib-combinator table, trait
  collection, and type-alias collection.

## [1.1.0] - 2026-04-24

Minor release: zero-annotation cross-adapter delegation check for
N-peer-adapter architectures (CLI + MCP + REST + …). No breaking
changes; the new check only fires when `[architecture.call_parity]`
is explicitly configured, and inert otherwise.

### Added
- **`[architecture.call_parity]`** — cross-adapter delegation drift
  check driven entirely by the existing `[architecture.layers]`
  configuration. No per-function annotation required: every `pub fn`
  in a configured adapter layer is checked automatically, and every
  new adapter handler participates in the check from its first commit.
  Two complementary rules run under one config section:
  - `architecture/call_parity/no_delegation` — each `pub fn` in an
    adapter layer must transitively (up to `call_depth` hops) call
    into the configured target layer. Catches inlined business logic.
  - `architecture/call_parity/missing_adapter` — each `pub fn` in
    the target layer must be transitively reached from every
    adapter layer. Catches asymmetric feature coverage (e.g. CLI
    + MCP both call `application::do_thing`, REST doesn't).
- **Receiver-type tracking** (`session.search(…)` resolution) — the
  call collector walks `let` bindings, signature parameters, and
  constructor returns to resolve method calls on Session / Service /
  Context objects. `Arc<T>`, `Box<T>`, `Rc<T>`, `&T`, `&mut T`,
  `Cow<'_, T>` wrappers are stripped. Critical for Session-pattern
  architectures, where method calls would otherwise stay
  `<method>:name` and the check would 100% false-positive.
- **`exclude_targets` glob escape** — legitimate asymmetric target
  fns (setup routines, debug-only endpoints) can be grouped under a
  glob pattern in the config, keeping the escape in one place instead
  of scattering `qual:allow(architecture)` markers across files.
- **`// qual:allow(architecture)`** as the secondary escape for
  individual fn-level asymmetries. Counts against
  `max_suppression_ratio` — overuse surfaces in the report.
- **`LayerDefinitions::layer_of_crate_path`** — resolves canonical
  call targets (`crate::a::b::c`) back to layer names. Internal API,
  reusable across future workspace-wide architecture rules.

### Infrastructure
- New `#[ignore]`-gated `benchmark_call_parity_on_self_analysis` test.
  Runs the full pipeline against rustqual's own ~200-file source tree
  and asserts the pass stays under a 3-second wall-time ceiling.
  Execute via `cargo test -- --ignored` before release.

## [1.0.1] - 2026-04-20

Patch release addressing five bugs reported against v0.5.6 (verified
against v1.0) plus one pre-existing CI gap uncovered during
investigation. No breaking changes; drop-in upgrade.

Self-analysis: `cargo run -- . --fail-on-warnings --coverage
coverage.lcov` reports 1913 functions, 100.0% quality score across all
7 dimensions, 0 findings. 1176 tests pass (35 new).

### Added
- **`// qual:test_helper` annotation** — narrow marker for
  integration-test helpers. Suppresses **only** the DRY-002 `testonly`
  dead-code finding and TQ-003 (`untested` production functions); all other
  checks (DRY duplicates, complexity, SRP, coupling, structural) keep
  applying. Does **not** count against `max_suppression_ratio`.
  Replaces the overly broad `ignore_functions` entry for the
  integration-test-helper use case.
- **Multi-line `qual:allow` rationale** — suppressions placed above a
  multi-line `//` comment block (a common pattern: marker on the first
  line, rationale on subsequent lines, then `#[derive]` + item) now
  work. The annotation window is measured from the block's last
  comment line, not the marker itself. Blank lines still break the
  block — misplaced markers don't reach their target.
- **Orphan-suppression findings** — `// qual:allow(...)` markers that
  match no finding in their annotation window are emitted as
  first-class `ORPHAN_SUPPRESSION` findings, visible in every output
  format (text, JSON, AI, SARIF, `--findings`). The AI format surfaces
  the marker's original reason string so the agent can tell whether
  it was a stale leftover or a misplaced annotation. Orphan findings
  contribute to `total_findings()` and thus to default-fail (they do
  not currently trigger `--fail-on-warnings`, which only gates on
  `suppression_ratio_exceeded`) — the user experience is: run
  rustqual, see the orphan in the list, delete or correct the marker,
  rerun. The
  detector reads raw complexity metrics (not the `*_warning` flags
  that suppressions clear), so a `// qual:allow(complexity)` marker
  on a genuinely over-threshold function is correctly recognized as
  non-orphan even after the suppression has silenced the user-visible
  finding. Coupling-only markers are skipped only when the file has
  no line-anchored Coupling finding to match by line window; when a
  line-anchored Coupling position exists (for example, a Structural
  warning with `dimension == Coupling`), the marker is verifiable.
- **`apply_parameter_warnings` marks suppressed entries instead of
  dropping them** — internal change that lets the orphan-suppression
  detector see SRP-param suppressions as matching targets. User-
  visible behavior unchanged (`srp_param_warnings` count still only
  tallies non-suppressed entries).

### Fixed
- **Test-companion files missed by cfg-test detection**. The
  `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;` pattern — common
  for co-locating unit tests next to their production module — was
  not recognized as cfg-test because (a) `ChildPathResolver` only
  tried the naming-convention paths (`foo/tests.rs`,
  `foo/tests/mod.rs`) and ignored the `#[path]` override, and (b)
  top-level `#![cfg(test)]` inner attributes on the companion file
  itself were never scanned. Both gaps closed: `#[path]` is now
  resolved relative to the parent file's directory (rustc
  semantics), and `file.attrs` is inspected for inner
  `#![cfg(test)]`. Fixes systematic SRP_MODULE false-positives on
  test-companion files whose many-test-one-cluster-each layout
  triggers `max_independent_clusters` by design.
- **Bug 2 — SRP LCOM4 false-positives via macro-wrapped method
  calls**. `MethodBodyVisitor` in the SRP cohesion analyzer now
  descends into macro token streams, so `self.method()` references
  inside `debug_assert!(...)`, `assert_eq!(...)`, `format!(...)`
  etc. count as inter-method edges. Paired reader/mutator patterns
  where a mutator calls a reader via `debug_assert!` are now
  correctly united into a single LCOM4 cluster.
- **Bug 4 — AI format omitted SRP_MODULE cluster driver**.
  `enrich_detail()` in the AI reporter now names both the length
  driver (`N lines (max M)`) and the cluster driver (`N independent
  clusters (max M)`) when either triggers, and combines both when
  both fire. Extended the same completeness discipline to six more
  finding categories: SDP (instability values), BOILERPLATE
  (description + suggested fix), DEAD_CODE (full suggestion text),
  STRUCTURAL (rule detail not just code), and kept the pre-existing
  enrichers for VIOLATION, DUPLICATE, FRAGMENT, SRP_STRUCT,
  COGNITIVE, CYCLOMATIC, LONG_FN, NESTING, SRP_PARAMS. Goal: a
  single `--format ai` invocation is always enough — no JSON
  fallback.
- **Bug 1 — DEAD_CODE/testonly suggestion was hard to act on**. The
  suggestion text now explicitly names both escape hatches:
  `// qual:api` (for truly public API functions) and
  `// qual:test_helper` (for test-only helpers in `src/`).
- **CI/release workflow self-analysis gap (pre-existing)** —
  `.github/workflows/ci.yml` and `release.yml` now run
  `cargo run -- . --fail-on-warnings --coverage coverage.lcov` with
  `.` as the analysis root (was `src/`). Architecture globs like
  `src/adapters/**` only match when paths are relative to the
  project root; running with `src/` stripped the prefix and silently
  disabled architecture-rule checking. The gap was uncovered when
  Bug 4's investigation revealed a forbidden-edge violation
  (`structural::oi` → `coupling::file_to_module`) that had been
  merged under this blind spot.
- **Pre-existing architecture violation** — moved `file_to_module`
  helper from `adapters::analyzers::coupling` to
  `adapters::shared::file_to_module`. Dimension analyzers now don't
  cross-import each other (forbidden-edge rule honored).

### Internal
- `cargo test` in CI/release replaced with `cargo nextest run` to
  match local-development discipline.
- New module `src/app/orphan_suppressions.rs` encapsulates the
  verification pass; `src/app/warnings.rs` shrank from 475 to ~270
  lines after the extraction.
- `run_dry_detection` signature refactored: the two annotation-line
  maps (`api` + `test_helper`) are passed as a single
  `AnnotationLines<'a>` struct to keep parameter count under the
  SRP_PARAMS threshold.

## [1.0.0] - 2026-04-20

Clean-Architecture refactor and seventh quality dimension, **fully
enforced** against rustqual's own codebase. **Breaking**: the
`[weights]` config schema now has 7 fields instead of 6 (new `architecture`
weight); projects with an explicit `[weights]` section must add it and
re-balance so the weights sum to 1.0.

Self-analysis: `cargo run -- . --fail-on-warnings --coverage coverage.lcov`
reports 1805 functions, 100.0% quality score across all 7 dimensions,
0 findings, 27 suppressions (qual:allow + `#[allow]`). 1114 tests pass.

### Added
- **Architecture dimension** — seventh quality dimension with four rule
  types: Layer Rule (rank-based import ordering), Forbidden Rule
  (from/to/except glob triplets), Symbol Patterns (7 matcher families:
  `forbid_path_prefix`, `forbid_glob_import`, `forbid_method_call`,
  `forbid_function_call`, `forbid_macro_call`, `forbid_item_kind`,
  `forbid_derive`), and Trait-Signature Rule (7 checks:
  `receiver_may_be`, `methods_must_be_async`, `forbidden_return_type_contains`,
  `required_param_type_contains`, `required_supertraits_contain`,
  `must_be_object_safe` conservative, `forbidden_error_variant_contains`).
- **`--explain <FILE>` CLI mode** — diagnostic output per file showing
  layer assignment, classified imports, and rule hits; makes config
  tuning in new repos tractable.
- **Golden example crates** at `examples/architecture/<rule>/` covering
  every matcher and rule with fixture + minimal rustqual.toml + snapshot
  test.

### Changed — Clean-Architecture refactor
- **Five-rank layered module structure** with explicit dependency
  direction (`domain → port → infrastructure → analysis → application`):
  - `src/domain/` — pure value types (`Dimension`, `Finding`,
    `Severity`, `SourceUnit`, `Suppression`, `PERCENTAGE_MULTIPLIER`).
    No `syn`, no I/O, no adapter-specific types.
  - `src/ports/` — trait contracts (`DimensionAnalyzer`, `SourceLoader`,
    `SuppressionParser`, `Reporter`). Carry `ParsedFile` DTOs.
  - `src/adapters/config/`, `src/adapters/source/`,
    `src/adapters/suppression/` — **infrastructure** adapters (I/O,
    TOML parsing, filesystem, suppression parsing).
  - `src/adapters/analyzers/` + `src/adapters/shared/` +
    `src/adapters/report/` — **analysis** layer: the seven dimension
    analyzers, their shared helpers (cfg-test detection, AST
    normalization, use-tree walker), and the eight report renderers.
    Reports sit at the same rank as analyzers so they may read rich
    analyzer DTOs (FunctionAnalysis, DeadCodeWarning) without
    ceremonial Finding-only projections.
  - `src/app/` — **application** use-cases: `pipeline` (full-pipeline
    orchestrator), `secondary` (per-dimension passes bundled through
    `SecondaryContext`), `metrics`/`tq_metrics`/`structural_metrics`
    (per-category helpers), `warnings` (complexity, leaf reclass,
    suppression ratio), `exit_gates`, `setup`, `analyze_codebase`
    (port-based).
  - `src/cli/` (`mod`, `handlers`, `explain`) + `src/main.rs` +
    `src/bin/cargo-qual/` + `src/lib.rs` + `tests/**` —
    composition root / re-export points.
- **Pipeline module dissolved** — the 1223-line `src/pipeline/` tree
  from the Phase-1–4 era is now fully absorbed into `src/app/`; the
  orchestrator is split between `pipeline.rs` (221 lines) and
  `secondary.rs` (179 lines, one helper per dimension pass).
- **Strict architecture enforcement** — `[architecture] enabled = true`,
  `unmatched_behavior = "strict_error"` (every production file must be
  in a layer). The full rule set runs in CI.
- **Workspace-root `tests/**` now analyzed** — previously excluded
  wholesale. Cargo's integration-test binaries are detected as
  test-only files by `adapters/shared/cfg_test_files`, so
  `is_test`-aware checks (LONG_FN, MAGIC_NUMBER, ERROR_HANDLING) skip
  them correctly while dead-code and structural checks still apply.
- **Test co-location** — every `#[cfg(test)] mod tests { … }` extracted
  into `<dir>/tests/<name>.rs` companions. Production files report
  honest length metrics (all < 500 lines, most < 300).
- **Architecture analyzer wired through the port** — first dimension to
  implement `DimensionAnalyzer`; `analyze_codebase` iterates
  `&[Box<dyn DimensionAnalyzer>]`.
- **7-dimension weights** (`[f64; 7]`): default
  `iosp=0.22, complexity=0.18, dry=0.13, srp=0.18, coupling=0.09,
  test_quality=0.10, architecture=0.10`.
- **`test` → `test_quality` rename** in `[weights]` config (old `test`
  field rejected with a deserialize error; migrate to `test_quality`).
- **`allow_expect = false`** by default — consistent with the
  architecture rule `no_panic_helpers_in_production`.

### Fixed
- **Cross-analyzer helper leakage** — `has_cfg_test`, `has_test_attr`,
  and `DeclaredFunction`-related cfg-test-file detection moved from
  `adapters/analyzers/dry/` into `adapters/shared/` so TQ and
  structural analyzers no longer import DRY internals.
- **Test-aware classification gap** — helper functions inside companion
  `tests/` subtrees weren't always flagged as `is_test=true` (only
  `#[test]`-attributed ones were). `Analyzer::with_cfg_test_files`
  now initialises `in_test=true` for every function in a cfg-test
  file, eliminating a class of false positives in complexity /
  error-handling checks.
- **Doc-duplicate `Config::load`** — `Config::load` now delegates to
  `Config::load_from_file` after an ancestor-search helper
  (`find_config_file`); removed the inline read+parse duplication.
- **Panic-helper redundancy** — 7 `.expect()` / `unwrap!` /
  `unreachable!` call sites in production code replaced with safe
  fallbacks (`GlobSet::empty()`, `layer_and_rank_for_file` pairing,
  `_ => continue` for non-exhaustive syn matches, `unwrap_or_else`
  for infallible JSON serialization).

## [0.5.6] - 2026-04-16

### Changed
- **Extracted TOON encoder into dedicated [`toon-encode`](https://github.com/SaschaOnTour/toon-encode) crate** for reuse in other projects. `src/report/ai.rs` now delegates to `toon_encode::encode_toon()` instead of hosting its own encoder.
- Removed ~280 lines of duplicated code from `ai.rs`: `encode_toon`, `is_tabular`, `encode_tabular`, `encode_list`, `toon_quote` + `INDENT`/`TOON_SPECIAL` constants + 18 pure encoder tests. Rustqual-specific enrichment (`build_ai_value`, `enrich_detail`, `map_category`) remains.
- Added `toon-encode` as a crates.io dependency (`toon-encode = "0.1"`).
- Test count: 882 — Function count: 488

## [0.5.5] - 2026-04-10

### Added
- **`--format ai` (TOON output)**: Token-optimized output for AI agents using [TOON format](https://toonformat.dev/). Findings are grouped by file (file paths appear once), categories use human-readable snake_case (`magic_number`, `duplicate`, `violation`), and details are enriched with actionable context (partner locations for duplicates/fragments, logic/call line numbers for violations, threshold values for complexity findings). ~66% fewer tokens than JSON.
- **`--format ai-json` (compact JSON)**: Same enriched structure as `--format ai` but serialized as JSON — fallback for AI tools that don't support TOON.
- Custom minimal TOON encoder (~80 lines, no new dependencies).
- `output_results()` now takes `&Config` instead of `&CouplingConfig`, enabling AI format to include threshold information in enriched details.
- 29 new tests for AI output (TOON encoder, category mapping, finding grouping, detail enrichment, serialization).
- Test count: 899 — Function count: 496

## [0.5.4] - 2026-04-10

### Fixed
- **Inconsistent findings count**: Summary header reported fewer findings than the Findings section. `total_findings()` counted magic numbers per-function (1) and duplicates/fragments/repeated matches per-group (1), while the findings list counted per-occurrence (2) and per-entry (2). Now both use per-occurrence/per-entry counting, making the numbers consistent.
- **Missing coupling findings in findings list**: Coupling threshold warnings and circular dependencies were counted in `total_findings()` but not emitted by `collect_all_findings()`. Added `warning: bool` flag on `CouplingMetrics` (set by `count_coupling_warnings`), new `COUPLING` and `CYCLE` categories in `collect_coupling_findings`.
- Extracted `count_dry_findings()` Operation in `pipeline/metrics.rs` to consolidate DRY entry counting and keep `run_secondary_analysis` under the function length threshold.
- Removed redundant pre-suppression counts for duplicates, fragments, and boilerplate in `run_dry_detection` (overwritten after suppression marking).
- 5 new consistency tests verifying `total_findings() == collect_all_findings().len()`.
- Test count: 868 — Function count: 477

## [0.5.3] - 2026-04-09

### Fixed
- **`./src/` path rejected on Windows**: The dot-directory filter excluded `.` (current directory) because `".".starts_with('.')` is true. Now skips hidden dirs (`.git`, `.tmp`) while preserving `.` and `..`.
- **OI false positives on Windows**: `top_level_module()` only split on `/`, causing backslash paths to be treated as different modules. Now normalizes `\` to `/`.
- **Internal path normalization**: `display_path` in `read_and_parse_files` and `rel` in `collect_filtered_files` now normalize backslashes at the source. Ensures consistent forward-slash paths across all dimensions and reports.
- **Empty location in findings**: Findings without file location (e.g. SDP) no longer render as `:0`.
- 4 new tests for path handling: dot-prefix path, hidden dir exclusion, target dir exclusion, forward-slash normalization.
- Test count: 862 — Function count: 476

## [0.5.2] - 2026-04-09

### Changed
- **Cleaner default output**: Summary shown first with total findings count in header line. File-grouped output only with `--verbose`. Default mode shows compact findings list with "═══ N Findings ═══" heading. Removed "Loaded config from ..." message, "N quality findings. Run with --verbose" footer, and file headers without context.
- **Coupling section**: Explanation text ("Incoming = modules depending on this one...") and "Modules analyzed: N" only shown with `--verbose`.
- **Windows path support**: Backslash paths (e.g., `.\src\` from PowerShell) are normalized to forward slashes on input.

### Fixed
- **OI false positives on Windows**: `top_level_module()` in the Orphaned Impl check only split on `/`, causing backslash paths like `db\queries\chunks.rs` to be treated as a different module than `db\connection.rs`. Now normalizes `\` to `/` before splitting. This caused 9 false OI findings on Windows that didn't appear on Linux/WSL.
- Test count: 858 — Function count: 476

## [0.5.1] - 2026-04-09

### Added
- **`// qual:allow(unsafe)` annotation**: Suppresses unsafe-block warnings on individual functions without affecting other complexity findings. Not parsed as a blanket suppression — does not count against suppression ratio.
- **Boilerplate suppression**: `BoilerplateFind` now has `suppressed: bool`. `qual:allow(dry)` on any boilerplate finding suppresses it. `DrySuppressible` trait extended with impl for `BoilerplateFind`.
- **SARIF BP-001..BP-010 rule definitions**: All 10 boilerplate patterns now have proper SARIF rule entries in `sarif_rules()`. SARIF ruleId uses `b.pattern_id` directly (e.g., `BP-003`).
- `is_within_window()` and `has_annotation_in_window()` utility functions in `findings.rs` — consolidates 5+ duplicated annotation-window check patterns.

### Fixed
- **BP-003 reports per getter, not per struct**: Each trivial getter/setter is now a separate finding on the function line, enabling `qual:allow(dry)` suppression per function.
- **`qual:allow(unsafe)` no longer parsed as blanket suppression**: Previously, `qual:allow(unsafe)` was silently treated as `qual:allow` (suppress all) because "unsafe" wasn't a recognized dimension. Now intercepted before suppression parsing.
- **SARIF boilerplate ruleId**: Was `BP-BP-003` (double prefix), now correctly `BP-003`.

### Changed
- `is_unsafe_allowed()` extracted as standalone function in `pipeline/warnings.rs`.
- `apply_extended_warnings()` accepts `unsafe_allow_lines` parameter.
- `pipeline/dry_suppressions.rs`: `DrySuppressible` impl for `BoilerplateFind`.
- Text/HTML DRY section headers respect suppressed state for all finding types.
- Test count: 857 — Function count: 475

## [0.5.0] - 2026-04-09

### Changed
- **BREAKING: Quality score formula rescaled**. The old formula dampened findings because each dimension independently divided by total analyzed functions. With 20 findings / 100 functions, the old score was ~90%; now it correctly reflects ~73%. Formula: `score = 1 - active_dims * (1 - weighted_avg)`, clamped to [0, 1]. Only active (non-zero weight) dimensions count. 100% is only achievable with 0 findings. 100% violations now scores 0% (was 75%).
- Test count: 852 — Function count: 468

## [0.4.6] - 2026-04-08

### Fixed
- **`qual:allow(dry)` now suppresses all DRY findings**: RepeatedMatchGroup (DRY-005) and FragmentGroup now have `suppressed: bool` fields. `qual:allow(dry)` on any member suppresses the finding. Previously only DuplicateGroup was suppressible.
- All 6 report formats filter suppressed fragments and repeated matches.

### Changed
- `DrySuppressible` trait + generic `mark_dry_suppressions()` replaces 3 duplicate suppression functions. Extracted to `pipeline/dry_suppressions.rs`.
- Test count: 849 — Function count: 468

## [0.4.5] - 2026-04-08

### Fixed
- **Struct field function pointers**: Bare function names in struct initialization (`Config { handler: my_function }`) are now recognized as usage by `CallTargetCollector` via `visit_expr_struct`. Fixes false-positive dead code warnings (DRY-003).

### Changed
- README: removed duplicate Recursive Annotation section.
- Test count: 847 — Function count: 462

## [0.4.4] - 2026-04-08

### Changed
- **Safe targets extended to non-Violations**: `apply_leaf_reclassification()` now treats ALL non-Violation functions as safe call targets — not just C=0 leaves. Calls to Integrations (L=0, C>0) no longer trigger Violations in the caller. Only calls to other Violations (mutually recursive or genuinely tangled functions) remain true Violations. This is a pragmatic IOSP relaxation documented in README.
- **`// qual:recursive` annotation**: Marks intentionally recursive functions. Self-calls are removed from own-call lists before reclassification. Does not count against suppression ratio.
- README: design note documenting safe-target reclassification as pragmatic IOSP relaxation.
- Test count: 844 — Function count: 459

## [0.4.2] - 2026-04-08

### Added
- **Automatic leaf detection**: Functions classified as Operation (C=0) or Trivial are automatically recognized as "leaves". Calls to leaf functions no longer count as own calls for the caller, eliminating false IOSP violations when mixing logic with calls to simple helpers (e.g., `get_config()`, `map_err()`). Iterates until stable for cascading leaf detection.
- `apply_leaf_reclassification()` in `pipeline/warnings.rs` — post-processing step that reclassifies Violations calling only leaves as Operations.
- 5 new unit tests for leaf detection (single leaf, multiple leaves, non-leaf still violation, pure integration unchanged, cascading).

### Changed
- Test count: 841 — Function count: 459
- Showcase and integration test fixtures updated to use non-leaf helpers where Violations are expected.

## [0.4.1] - 2026-04-08

### Added
- **Type-aware method-call resolution**: `.method()` calls now use receiver type info (self type, parameter types) to determine if a call is own or external. Eliminates false-positive IOSP violations from std method name collisions.
- `methods_by_type` on `ProjectScope`, `extract_param_types()`, `resolve_receiver_type()`, `is_type_resolved_own_method()` on `BodyVisitor`.
- **PascalCase enum variant exclusion**: `Type::Variant(...)` not counted as own calls.

### Changed
- **BREAKING: `external_prefixes` removed** from config. Type-aware resolution replaces manual prefix lists. Remove `external_prefixes` from `rustqual.toml` to fix.
- **BREAKING: `UNIVERSAL_METHODS` removed**. `trait_only_methods` + type-aware resolution handle all cases.
- `classify_function()` accepts `type_context` tuple for receiver resolution.
- `BodyVisitor` gains `parent_type` and `param_types` fields.
- Test count: 836 — Function count: 458

## [0.4.0] - 2026-04-08

### Added
- **`// qual:inverse(fn_name)` annotation**: Marks inverse method pairs (e.g., `as_str`/`parse`, `encode`/`decode`). Suppresses near-duplicate DRY findings between paired functions without counting against the suppression ratio. Parsed by `parse_inverse_marker()` in `findings.rs`, collected by `collect_inverse_lines()` in `pipeline/discovery.rs`.
- **`qual:allow(dry)` suppression for duplicate groups**: `// qual:allow(dry)` on any member of a duplicate pair now correctly suppresses the finding. Previously only single-function findings were suppressible.
- `suppressed: bool` field on `DuplicateGroup` — enables per-group suppression.
- `mark_duplicate_suppressions()` and `mark_inverse_suppressions()` in `pipeline/metrics.rs`.
- **LCOM4 self-method-call resolution**: Methods calling `self.conn()` now transitively share the field accesses of the called method. `self_method_calls` tracked per method, resolved one level deep in `build_field_method_index()`. Fixes false high LCOM4 for types using accessor methods.
- `self_method_calls: HashSet<String>` field on `MethodFieldData`.
- `build_field_method_index()` extracted as Operation in `srp/cohesion.rs`.
- `collect_per_file()` generic helper in `pipeline/discovery.rs` — eliminates near-duplicate code in `collect_suppression_lines`, `collect_api_lines`, `collect_inverse_lines`.
- 20 new unit tests across all fixed areas.

### Fixed
- **`#[cfg(test)] impl` propagation**: Methods inside `#[cfg(test)] impl Type { ... }` blocks are now correctly recognized as test code (`in_test = true`). Fixes DRY-003 false positives for test helpers in cfg-test impl blocks. Both `DeclaredFnCollector` and `FunctionCollector` (dry) and the IOSP analyzer now propagate the flag.
- **`matches!(self, ...)` SLM detection**: The SLM (Self-less Methods) check now recognizes `matches!(self, ...)` as a self-reference by inspecting macro token streams. Previously flagged as "self never referenced".
- **`qual:api` TQ-003 pipeline fix**: `compute_tq()` now calls `mark_api_declarations()` on its declared functions, so `// qual:api` correctly excludes functions from untested-function detection. Previously, TQ analysis collected fresh `DeclaredFunction` objects without API markings.
- **Function pointer references in dead code**: `&function_name` passed as an argument is now recognized as a usage by `CallTargetCollector`. `record_path_args()` unwraps `Expr::Reference` to extract the inner path.
- **Enum variant constructors**: `ChunkKind::Other(...)`, `RefKind::Call` etc. no longer counted as own calls (PascalCase heuristic).
- **Error-handling dispatch**: `match op() { Ok(r) => ..., Err(e) => ... }` patterns benefit from the type-aware resolution — std method calls in arms no longer flagged.
- All 6 report formats (text, JSON, SARIF, HTML, GitHub annotations, findings list) now filter suppressed duplicate groups.

### Changed
- **BREAKING: `external_prefixes` removed** from config. Type-aware method resolution replaces the manual prefix lists. Old `rustqual.toml` files with `external_prefixes` will error — remove the field to fix.
- **BREAKING: `UNIVERSAL_METHODS` removed** from scope. `trait_only_methods` + type-aware resolution handle all cases previously covered by the hardcoded list.
- **SRP refactoring**: `FunctionCollector` moved from `dry/mod.rs` to `dry/functions.rs`, `DeclaredFnCollector` moved to `dry/dead_code.rs`. Reduces `dry/mod.rs` production lines from 304 to ~125.
- `mark_api_declarations()` changed from private to `pub(crate)`, signature changed to `&mut [DeclaredFunction]` (was by-value).
- `classify_function()` accepts `type_context: (Option<&str>, &Signature)` for receiver type resolution.
- `BodyVisitor` gains `parent_type` and `param_types` fields for type-aware method classification.
- Test count: 836 tests (829 unit + 4 integration + 3 showcase)
- Function count: 458

## [0.3.9] - 2026-04-02

### Fixed
- **Stacked annotations**: Multiple `// qual:*` annotations before a function now all work (e.g., `// qual:api` + `// qual:allow(iosp)`). Expanded adjacency window from 1 line to 3 lines (`ANNOTATION_WINDOW` constant in `findings.rs`).
- **NMS false positive**: `self.field[index].method()` (indexed field method call) is now correctly recognized as a mutation of `&mut self`. Previously only `self.field.method()` was detected.

## [0.3.6] - 2026-03-29

### Added
- **`// qual:api` annotation**: Mark public API functions to exclude them from dead code detection (DRY-003) and untested function detection (TQ-003) without counting against the suppression ratio. API functions are meant to be called by external consumers and may be tested via integration tests outside the project.
- `is_api: bool` field on `DeclaredFunction` — tracks whether a function has a `// qual:api` marker.
- `is_api_marker()` in `findings.rs` — parses `// qual:api` comments.
- `collect_api_lines()` in `pipeline/discovery.rs` — collects API marker line numbers per file.
- `mark_api_declarations()` in `dry/dead_code.rs` — marks declared functions with API annotations.
- 7 new unit tests for API marker parsing, dead code exclusion, and suppression non-counting.
- **`--findings` CLI flag**: One-line-per-finding output with `file:line category detail in function_name`, sorted by file and line. Ideal for CI integration and quick diagnosis.
- **Summary inline locations**: When total findings ≤ 10, the summary shows `→ file:line (detail)` sub-lines under each dimension with findings, making locations visible without `--verbose`.
- **TRIVIAL findings visible**: `--verbose` now shows `⚠` warning lines for TRIVIAL functions that have findings (magic numbers, complexity, etc.) — previously these were hidden.
- `FindingEntry` struct and `collect_all_findings()` in `report/findings_list.rs` — unified finding collection reused by both `--findings` and summary locations.
- 5 new unit tests for `collect_all_findings()`.

### Changed
- `detect_dead_code()` now accepts `api_lines` parameter for API exclusion.
- `should_exclude()` checks `d.is_api` alongside `is_main`, `is_test`, etc.
- `detect_untested_functions()` (TQ-003) excludes API-marked functions.
- Test count: 821 tests (814 unit + 4 integration + 3 showcase)
- Function count: 441

## [0.3.5] - 2026-03-29

### Added
- **Test-aware IOSP analysis**: Functions with `#[test]` attribute or inside `#[cfg(test)]` modules are now automatically recognized as test code. IOSP violations in test functions are reclassified as Trivial — tests inherently mix calls and assertions (Arrange-Act-Assert pattern), which is not a design defect.
- **Test-aware error handling**: `unwrap()`, `panic!()`, `todo!()`, and `expect()` in test functions no longer produce error-handling findings. These are idiomatic Rust test patterns.
- `is_test: bool` field on `FunctionAnalysis` — tracks whether a function is test code.
- `exclude_test_violations()` pipeline function — reclassifies test violations before counting.
- `has_error_handling_issue()` extracted as standalone Operation for IOSP compliance.
- `finalize_summary()` extracted from `run_analysis()` for IOSP compliance.
- 7 new unit tests for `is_test` detection, test violation exclusion, and error handling gating.
- **Array index magic number exclusion**: Numeric literals inside array index expressions (`values[3]`, `matrix[3][4]`) are no longer flagged as magic numbers. Array indices are positional — the index IS the meaning. Uses `in_index_context` depth counter (same pattern as `in_const_context`). 3 new unit tests.

### Changed
- `has_test_attr()` and `has_cfg_test()` promoted from `pub(super)` to `pub(crate)` in `dry/mod.rs` for reuse in analyzer.
- Test count: 809 tests (802 unit + 4 integration + 3 showcase)
- Function count: 426

## [0.3.4] - 2026-03-26

### Fixed
- **TQ-003 false positive** for functions called only inside macro invocations (`assert!()`, `assert_eq!()`, `format!()`, etc.) — `CallTargetCollector` now parses macro token streams as comma-separated expressions, extracting embedded function calls for both `test_calls` and `production_calls`. Same pattern as `TestCallCollector` in `sut.rs`. This also fixes potential false positives in dead code detection (DRY-003/DRY-004) where production calls inside macros were missed.

### Changed
- Test count: 799 tests (792 unit + 4 integration + 3 showcase)

## [0.3.3] - 2026-03-26

### Added
- **DRY-005: Repeated match pattern detection** — detects identical `match` blocks (≥3 arms, ≥3 instances across ≥2 functions) by normalizing and hashing match expressions. New file `src/dry/match_patterns.rs` with `MatchPatternCollector` visitor, `detect_repeated_matches()` Integration, and `group_repeated_patterns()` Operation. Enum name is extracted from arm patterns (best effort).
- `detect_repeated_matches` field in `[duplicates]` config (default: `true`)
- DRY-005 output in all 6 report formats (text, JSON, GitHub, HTML, SARIF, dot)
- `StructuralWarningKind::code()` and `StructuralWarningKind::detail()` methods — centralizes the `(code, detail)` extraction that was previously duplicated across 5 report files

### Changed
- `print_dry_section` and `print_dry_annotations` now take `&AnalysisResult` instead of 6 separate slice parameters, matching the pattern used by `print_json` and `print_html`
- 5 report files (text/structural, json_structural, github, html/structural_table, sarif/structural_collector) refactored to use `code()`/`detail()` methods instead of duplicated match blocks
- Test count: 797 tests (790 unit + 4 integration + 3 showcase)
- Function count: 422

## [0.3.2] - 2026-03-26

### Removed
- **SSM (Scattered Match) structural check** — redundant with DRY fragment detection and Rust's exhaustive matching. SSM produced false positives in most real-world cases (7/10 not actionable) and rustqual itself required 8 enums in `ssm_exclude_enums`. The `check_ssm` and `ssm_exclude_enums` config options have been removed.

### Changed
- Structural binary checks reduced from 8 to 7 rules (BTC, SLM, NMS, OI, SIT, DEH, IET)
- Test count: 787 tests (780 unit + 4 integration + 3 showcase)
- Function count: 412

## [0.3.1] - 2026-03-26

### Fixed
- **BP-006 false positive on or-patterns** — `match` arms with `Pat::Or` (e.g. `A | B => ...`) are no longer flagged as repetitive enum mapping boilerplate. The new `is_simple_enum_pattern()` rejects or-patterns, top-level wildcards, tuple patterns, and variable bindings.
- **BP-006 false positive on dispatch with bindings** — `match` arms that bind variables (e.g. `Msg::A(x) => handle(x)`) are no longer flagged. Only unit variants (`Color::Red`) and tuple-struct variants with wildcard sub-patterns (`Action::Add(_)`) are accepted as repetitive mapping patterns.
- **BP-006 false positive on tuple scrutinees** — `match (a, b) { ... }` expressions are now skipped by the repetitive match detector, since tuple scrutinees indicate multi-variable dispatch, not enum-to-enum mapping.
- **TQ-001 false positive on custom assertion macros** — `assert_relative_eq!`, `assert_approx_eq!`, and all other `assert_*`/`debug_assert_*` macros are now recognized via prefix matching instead of exact-match against a hardcoded list. For non-assert-prefixed macros (e.g. `verify!`), use the new `extra_assertion_macros` config option.

### Added
- `extra_assertion_macros` field in `[test]` config — list of additional macro names to treat as assertions for TQ-001 detection (for macros that don't start with `assert` or `debug_assert`)

### Changed
- `is_all_path_arms()` renamed to `is_repetitive_enum_mapping()` with stricter pattern validation (guards, or-patterns, wildcards, and variable bindings now rejected)
- Test count: 790 tests (783 unit + 4 integration + 3 showcase)
- Function count: 417

## [0.3.0] - 2026-03-25

### Added

#### Structural Binary Checks (8 rules)
- **BTC (Broken Trait Contract)** — flags impl blocks that are missing required trait methods (SRP dimension)
- **SLM (Self-less Methods)** — flags methods in impl blocks that don't use `self` and could be free functions (SRP dimension)
- **NMS (Needless &mut self)** — flags methods that take `&mut self` but only read from self (SRP dimension)
- **SSM (Scattered Match)** — flags enums matched in 3+ separate locations, suggesting missing method on enum (SRP dimension) *(removed in 0.3.2)*
- **OI (Orphaned Impl)** — flags impl blocks in files that don't define the type they implement (Coupling dimension)
- **SIT (Single-Impl Trait)** — flags traits with exactly one implementation, suggesting unnecessary abstraction (Coupling dimension)
- **DEH (Downcast Escape Hatch)** — flags usage of `.downcast_ref()` / `.downcast_mut()` / `.downcast()` indicating broken abstraction (Coupling dimension)
- **IET (Inconsistent Error Types)** — flags modules returning 3+ different error types, suggesting missing unified error type (Coupling dimension)
- Integrated into existing SRP and Coupling dimensions (no new quality dimension)
- `[structural]` config section with `enabled` and per-rule `check_*` bools
- New module: `structural/` with `mod.rs`, `btc.rs`, `slm.rs`, `nms.rs`, `oi.rs`, `sit.rs`, `deh.rs`, `iet.rs`
- New pipeline module: `pipeline/structural_metrics.rs`
- New report module: `report/text/structural.rs`
- All report formats updated with structural findings

#### New Quality Dimension: Test Quality (TQ)
- **TQ-001 No Assertion** — flags `#[test]` functions with no assertion macros (`assert!`, `assert_eq!`, `assert_ne!`, `debug_assert!*`). `#[should_panic]` + `panic!` counts as assertion.
- **TQ-002 No SUT Call** — flags `#[test]` functions that don't call any production function (only external/std calls)
- **TQ-003 Untested Function** — flags production functions called from prod code but never from any test
- **TQ-004 Uncovered Function** — flags production functions with 0 execution count in LCOV coverage data (requires `--coverage`)
- **TQ-005 Untested Logic** — flags production functions with logic occurrences (if/match/for/while) at lines uncovered in LCOV data. Combines rustqual's structural analysis with coverage data. One warning per function with details of uncovered logic lines. (requires `--coverage`)

#### LCOV Coverage Integration
- **`--coverage <LCOV_FILE>`** CLI flag — ingest LCOV coverage data for TQ-004 and TQ-005 checks
- **LCOV parser** — parses `SF:`, `FNDA:`, `DA:` records; graceful handling of malformed lines

#### Configuration
- **`[test]` config section** — `enabled` (default true), `coverage_file` (optional LCOV path)
- **6-field `[weights]` section** — new `test` weight field; default weights redistributed: `[0.25, 0.20, 0.15, 0.20, 0.10, 0.10]` for [IOSP, CX, DRY, SRP, CP, TQ]
- **`Dimension::Test`** — new dimension variant, parseable as `"test"` or `"tq"`, suppressible via `// qual:allow(test)`

#### Report Formats
- All report formats updated: text, JSON, GitHub annotations, HTML dashboard (6th card), SARIF (TQ-001..005 rules), baseline (TQ fields with backward compat)

### Changed
- **Breaking**: Default quality weights redistributed from 5 to 6 dimensions. Existing configs with explicit `[weights]` sections must add `test = 0.10` and adjust other weights to sum to 1.0.
- `ComplexityMetrics` now includes `logic_occurrences: Vec<LogicOccurrence>` for TQ-005 coverage analysis
- `extract_init_metrics()` moved from `lib.rs` to `config/init.rs`
- Version bump: 0.2.0 → 0.3.0
- Test count: 774 tests (767 unit + 4 integration + 3 showcase)
- Function count: 402

### Fixed
- **SDP violations not respecting `qual:allow(coupling)` suppressions** — `SdpViolation` now has a `suppressed: bool` field. `mark_sdp_suppressions()` in pipeline/metrics.rs sets it when either the `from_module` or `to_module` has a coupling suppression. `count_sdp_violations()` filters suppressed entries. All report formats (text, JSON, GitHub, SARIF, HTML) skip suppressed SDP violations.
- **Serde `deserialize_with`/`serialize_with` functions falsely flagged as dead code** — `CallTargetCollector` now implements `visit_field()` to extract function references from `#[serde(deserialize_with = "fn")]`, `#[serde(serialize_with = "fn")]`, `#[serde(default = "fn")]`, and `#[serde(with = "module")]` attributes. The new `extract_serde_fn_refs()` static method parses serde attribute metadata and registers both bare and qualified function names as call targets.
- **Trait method calls on parameters falsely classified as own calls** — Methods that only appear in trait definitions or `impl Trait for Struct` blocks (never in inherent `impl Struct` blocks) are now tracked as "trait-only" methods. Dot-syntax calls to these methods (e.g. `provider.fetch_daily_bars()`) are recognized as polymorphic dispatch, not own calls, preventing false IOSP Violations. Conservative: if a method name appears in both trait and inherent impl contexts, it is still counted as an own call.
- **Dead code false positives on `#[cfg(test)] mod` files** — Functions in files loaded via `#[cfg(test)] mod helpers;` (external module declarations) are no longer falsely flagged as "test-only" or "uncalled" dead code. The new `collect_cfg_test_file_paths()` scans parent files for `#[cfg(test)] mod name;` declarations and computes child file paths. `mark_cfg_test_declarations()` marks functions in those files as test code, and `collect_all_calls()` initializes `in_test = true` for cfg-test files so calls from them are classified as test calls. Supports both `name.rs` and `name/mod.rs` child layouts, and non-mod parent files (`foo.rs` → `foo/name.rs`).
- **Dead code false positives on `pub use` re-exports** — Functions exclusively accessed via `pub use` re-exports (with or without `as` rename, including grouped imports) are no longer falsely reported as uncalled dead code. The `CallTargetCollector` now implements `visit_item_use()` to record re-exported names. Private `use` imports are correctly skipped (calls captured via `visit_expr_call`). Glob re-exports (`pub use foo::*`) are conservatively skipped.
- **For-loop delegation false positives** — `for x in items { call(x); }` is no longer flagged as a Violation. For-loops with delegation-only bodies (calls, `let` bindings with calls, `?` on calls, `if let` with call scrutinee) are treated equivalently to `.for_each()` in lenient mode. Complexity metrics are still tracked. Detection uses `is_delegation_only_body()` with iterative stack-based AST analysis split into `extract_delegation_exprs` + `check_delegation_stack` for IOSP self-compliance.
- **Trivial self-getter false positives** — Methods like `fn count(&self) -> usize { self.items.len() }` are now detected as trivial accessors and excluded from own-call counting. This prevents Operations that call trivial getters from being misclassified as Violations. Detection supports field access, `&self.x`, stdlib accessor chains (`.len()`, `.clone()`, `.as_ref()`, etc.), casts, and unary operators. Name collisions across impl blocks are handled conservatively (non-trivial wins).
- **Type::new() false-positive own-call** — `Type::new()`, `Type::default()`, `Type::from()` and other universal methods called with a project-defined type prefix are no longer counted as own calls. Previously, `UNIVERSAL_METHODS` filtering was only applied to `Self::method` calls but not `Type::method` calls, causing false Violations when e.g. `Adx::new(14)` appeared alongside logic.
- **Trivial .get() accessor not recognized** — Methods like `fn current(&self) -> Option<&T> { self.items.get(self.index) }` are now detected as trivial accessors. The `.get()` method with a trivial argument (literal, self field access, or reference thereof) is recognized by the new `is_trivial_method_call()` helper, which was split from `is_trivial_accessor_body()` to keep cyclomatic complexity under threshold.
- **Match-dispatch false positives** — `match x { A => call_a(), B => call_b() }` is no longer flagged as a Violation. Match expressions where every arm is delegation-only (calls, method calls, `?`, blocks with delegation statements) and has no guard are treated as pure dispatch/routing — conceptually an Integration. Analogous to the for-loop delegation fix. Complexity metrics (cognitive, cyclomatic, hotspots) are still always tracked. Arms with guards (`x if x > 0 =>`) or logic (`a + b`) correctly remain Violations.

## [0.2.0] - 2026-02-26

### Added

#### New Complexity Checks
- **CX-004 Function Length** — warns when a function body exceeds `max_function_lines` (default 60)
- **CX-005 Nesting Depth** — warns when nesting depth exceeds `max_nesting_depth` (default 4)
- **CX-006 Unsafe Detection** — flags functions containing `unsafe` blocks (`detect_unsafe`, default true)
- **A20 Error Handling** — detects `.unwrap()`, `.expect()`, `panic!`, `todo!`, `unreachable!` usage (`detect_error_handling`, default true; `allow_expect`, default false)

#### New SRP Check
- **SRP-004 Parameter Count** — AST-based parameter counting replaces text-scanning `#[allow(clippy::too_many_arguments)]` detection; configurable `max_parameters` (default 5), excludes trait impls

#### New DRY Checks
- **A11 Wildcard Imports** — flags `use foo::*` imports (excludes `prelude::*`, `super::*` in test modules); configurable `detect_wildcard_imports`
- **A10 Boilerplate** — BP-009 (struct update syntax repetition) and BP-010 (format string repetition) pattern stubs

#### New Coupling Check
- **A16 Stable Dependencies Principle (SDP)** — flags when a stable module depends on a more unstable module; configurable `check_sdp`

#### New Tool Extensions
- **A2 Effort Score** — refactoring effort score for IOSP violations: `effort = logic*1.0 + calls*1.5 + nesting*2.0`; sort violations by effort with `--sort-by-effort`
- **E5 Configurable Quality Weights** — `[weights]` section in `rustqual.toml` with per-dimension weights (must sum to 1.0); validation on load
- **E6 Diff-Based Analysis** — `--diff [REF]` flag analyzes only files changed vs a git ref (default HEAD); graceful fallback for non-git repos
- **E9 Improved Init** — `--init` now runs a quick analysis to compute tailored thresholds (current max + 20% headroom) instead of using static defaults

#### Other
- `--fail-on-warnings` CLI flag — treats warnings (e.g. suppression ratio exceeded) as errors (exit code 1), analogous to clippy's `-Dwarnings`
- `fail_on_warnings` config field in `rustqual.toml` (default: `false`)
- Result-based error handling: all quality gate functions return `Result<(), i32>` instead of calling `process::exit()`, enabling unit tests for error paths
- `lib.rs` extraction: all logic moved to `src/lib.rs` with `pub fn run() -> Result<(), i32>`, binaries are thin wrappers
- New IOSP-compliant sub-functions: `determine_output_format()`, `check_default_fail()`, `setup_config()`, `apply_exit_gates()`
- `apply_file_suppressions()` in pipeline/warnings.rs for IOSP-safe suppression application
- `run_dry_detection()` in pipeline/metrics.rs for IOSP-safe DRY orchestration

### Changed
- Binary targets use Cargo auto-discovery (`src/main.rs` → `rustqual`, `src/bin/cargo-qual/main.rs` → `cargo-qual`) instead of explicit `[[bin]]` sections pointing to the same file — eliminates "found to be present in multiple build targets" warning
- Unit tests now run once (lib target) instead of twice (per binary target)
- `compute_severity()` now public (removed `#[cfg(test)]`), replacing inlined severity logic in `build_function_analysis` with a closure call
- HTML sections, text report, GitHub annotations, SARIF, and pipeline functions refactored to stay under 60-line function length threshold

### Fixed
- `count_all_suppressions()` attribute ordering bug: `#[allow(...)]` attributes directly before `#[cfg(test)]` were incorrectly counted as production code. Now uses backward walk to exclude test module attribute groups.
- CLI about string: "six dimensions" → "five dimensions"
- `cargo fmt` applied to `examples/sample.rs`

## [0.1.0] - 2026-02-22

### Added
- Five-dimension quality analysis: IOSP, Complexity, DRY, SRP, Coupling
- Weighted quality score (0-100%) with configurable dimension weights
- 6 output formats: text, json, github, dot, sarif, html
- Inline suppression: `// qual:allow`, `// qual:allow(dim)`, legacy `// iosp:allow`
- Default-fail behavior (exit 1 on findings, `--no-fail` for local use)
- Configuration via `rustqual.toml` with auto-discovery
- Watch mode (`--watch`): re-analyze on file changes
- Baseline comparison (`--save-baseline`, `--compare`, `--fail-on-regression`)
- Shell completions for bash, zsh, fish, elvish, powershell
- Dual binary: `rustqual` (direct) and `cargo qual` (cargo subcommand)
- Refactoring suggestions (`--suggestions`) for IOSP violations
- Quality gates (`--min-quality-score`)
- Complexity analysis: cognitive/cyclomatic metrics, magic number detection
- DRY analysis: duplicate functions, duplicate fragments, dead code, boilerplate (BP-001 through BP-010)
- SRP analysis: struct-level LCOM4 cohesion, module-level line length, function cohesion clusters
- Coupling analysis: afferent/efferent coupling, instability, circular dependency detection (Kosaraju SCC)
- Self-contained HTML report with dashboard and collapsible sections
- SARIF v2.1.0 output for GitHub Code Scanning integration
- GitHub Actions annotations format
- DOT/Graphviz call-graph visualization
- CI pipeline (GitHub Actions): fmt, clippy (`-Dwarnings`), test, self-analysis
- Release pipeline: cross-compiled binaries (6 targets), crates.io publish, GitHub Release

### Changed
- Replaced `#[allow(clippy::field_reassign_with_default)]` suppressions with struct literal syntax across 8 test modules
- Replaced `Box::new(T::default())` with `Box::default()` in analyzer visitor tests
- Added `#[derive(Default)]` to `ProjectScope` for cleaner test construction
- Clippy is now documented as running with `RUSTFLAGS="-Dwarnings"` (CI-equivalent)

[0.3.0]: https://github.com/SaschaOnTour/rustqual/releases/tag/v0.3.0
[0.2.0]: https://github.com/SaschaOnTour/rustqual/releases/tag/v0.2.0
[0.1.0]: https://github.com/SaschaOnTour/rustqual/releases/tag/v0.1.0
