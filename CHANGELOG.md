# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.5] - 2026-04-10

### Added
- **`--format ai` (TOON output)**: Token-optimized output for AI agents using [TOON format](https://toonformat.dev/). Findings are grouped by file (file paths appear once), categories use human-readable snake_case (`magic_number`, `duplicate`, `violation`), and details are enriched with actionable context (partner locations for duplicates/fragments, logic/call line numbers for violations, threshold values for complexity findings). ~66% fewer tokens than JSON.
- **`--format ai-json` (compact JSON)**: Same enriched structure as `--format ai` but serialized as JSON — fallback for AI tools that don't support TOON.
- New dependency: `toon-format` v0.2 (official TOON encoder).
- `output_results()` now takes `&Config` instead of `&CouplingConfig`, enabling AI format to include threshold information in enriched details.
- 20 new tests for AI output (category mapping, finding grouping, detail enrichment, TOON/JSON serialization).
- Test count: 888 — Function count: 491

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
