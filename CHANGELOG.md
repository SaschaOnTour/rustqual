# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- **Type-alias expansion.** `type Db = Arc<RwLock<Store>>;` recorded
  in the workspace index; `fn h(db: Db) { db.read() }` expands `Db`
  → `Arc<RwLock<Store>>` → `Store` (Arc/RwLock peeled by the stdlib
  wrapper rules) and resolves `read` against Store's method index.
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
- `let (a, s) = make_pair(); s.m()` — tuple destructuring. Tuple
  element types aren't tracked.
- `for item in xs { item.m() }` — for-loop pattern binding doesn't
  flow into the method-call collector yet. `item` stays unbound.
- `match res { Ok(s) => s.m(), … }` — `match`-arm pattern bindings
  aren't wired into the scope stack. Use `let` or `if let` as
  workarounds.
- `Session::open().map(|r| r.m())` — closure-body argument type is
  unknown. Inner method call stays `<method>:m`.
- `fn get<T>() -> T { … }; let x = get(); x.m()` without annotation
  or turbofish. Use `let x: T = get();` or `get::<T>()`.
- `fn make() -> impl Trait { … }; make().inherent_method()` —
  `impl Trait` hides the concrete type by design. Only trait methods
  are resolvable (via trait-dispatch over-approximation).
- Arbitrary proc-macros that alter the call graph without being in
  `transparent_macros` config. User-annotate via
  `// qual:allow(architecture)` on the enclosing fn.

### Infrastructure
- **`tests/rlm_snapshot.rs`** — end-to-end regression snapshot with a
  3-file rlm-shape fixture (application/session, cli/handlers,
  mcp/handlers). Asserts a budget of **0 Check A findings + 5 Check B
  findings** (the 5 legitimate asymmetries / dead-code items). Any
  drift in this count is a clear regression signal.
- **`tests/regressions.rs`** — unit-level tests covering every rlm
  Gruppe-2 / Gruppe-3 pattern plus Stage-2 trait-dispatch /
  turbofish cases and Stage-3 type-alias / user-wrapper cases.
  Negative tests pin documented limits in place.
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
