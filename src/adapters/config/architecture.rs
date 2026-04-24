//! Architecture-Dimension configuration structs.
//!
//! These structs deserialize the `[architecture]` section of rustqual.toml
//! into Rust values. They are parse-only in v0.x — the Architecture-Dimension
//! tool features are implemented progressively across phases, and the config
//! structs exist from the start so the complete target architecture can be
//! expressed in rustqual.toml from day one.
//!
//! Rule groups:
//! - Layer rule: [architecture.layers] + [architecture.layers.<name>] + [architecture.external_crates]
//! - Re-export policy: [architecture.reexport_points]
//! - Forbidden edges: [[architecture.forbidden]]
//! - Symbol rules: [[architecture.pattern]]
//! - Trait-signature rules: [[architecture.trait_contract]]

// Fields on these structs are deserialized but not yet read — the architecture
// analyzer that consumes them is implemented progressively across Phases 1–9.
// Tests exercise parsing (the public contract we care about in Phase 0).
#![allow(dead_code)]

use serde::Deserialize;
use std::collections::HashMap;

/// Root `[architecture]` config section.
#[derive(Debug, Default, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct ArchitectureConfig {
    /// Master switch for the Architecture dimension. When `false`, all rules
    /// below are ignored and the dimension contributes score 1.0 with zero
    /// findings.
    pub enabled: bool,

    /// Layer definitions: `[architecture.layers]`.
    pub layers: ArchitectureLayersConfig,

    /// Re-export points: `[architecture.reexport_points]`.
    pub reexport_points: ReexportPointsConfig,

    /// External-crate layer mapping: `[architecture.external_crates]`.
    /// Keys are crate names (glob patterns allowed), values are layer names.
    pub external_crates: HashMap<String, String>,

    /// Forbidden edges: `[[architecture.forbidden]]`.
    #[serde(rename = "forbidden")]
    pub forbidden_rules: Vec<ForbiddenRule>,

    /// Symbol rules: `[[architecture.pattern]]`.
    #[serde(rename = "pattern")]
    pub patterns: Vec<SymbolPattern>,

    /// Trait-signature rules: `[[architecture.trait_contract]]`.
    #[serde(rename = "trait_contract")]
    pub trait_contracts: Vec<TraitContract>,

    /// Call-parity check: `[architecture.call_parity]`.
    ///
    /// When `Some`, enforces that peer adapter layers delegate to a shared
    /// target layer (Check A) and that target-layer pub fns are reached from
    /// every adapter layer (Check B). When `None`, the check is inert.
    #[serde(default)]
    pub call_parity: Option<CallParityConfig>,
}

/// `[architecture.layers]` — layer order + per-layer path definitions.
///
/// The `order` lists layer names from innermost (lowest rank, 0) to outermost
/// (highest rank). Dynamic per-layer path maps like `[architecture.layers.domain]`
/// are captured in `definitions` via `#[serde(flatten)]`.
#[derive(Debug, Deserialize, Clone)]
pub struct ArchitectureLayersConfig {
    /// Layer names from innermost to outermost.
    #[serde(default)]
    pub order: Vec<String>,

    /// Behavior for files not matched by any layer's `paths` glob.
    /// Allowed values: "composition_root" | "strict_error".
    #[serde(default = "default_unmatched_behavior")]
    pub unmatched_behavior: String,

    /// Per-layer path globs (dynamically named, e.g. `[architecture.layers.domain]`).
    #[serde(flatten)]
    pub definitions: HashMap<String, LayerPathsConfig>,
}

impl Default for ArchitectureLayersConfig {
    fn default() -> Self {
        Self {
            order: Vec::new(),
            unmatched_behavior: default_unmatched_behavior(),
            definitions: HashMap::new(),
        }
    }
}

fn default_unmatched_behavior() -> String {
    "composition_root".to_string()
}

/// `[architecture.layers.<name>]` — path globs assigning files to a layer.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct LayerPathsConfig {
    /// File-path glob patterns that belong to this layer.
    pub paths: Vec<String>,
}

/// `[architecture.reexport_points]` — files that may freely re-export across layers.
///
/// Default without this section: only `src/lib.rs` and `src/main.rs` are treated
/// as re-export points. Other files must obey layer rules even for `pub use`.
#[derive(Debug, Deserialize, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct ReexportPointsConfig {
    /// File-path globs for files permitted as re-export points.
    pub paths: Vec<String>,
}

impl Default for ReexportPointsConfig {
    fn default() -> Self {
        Self {
            paths: vec!["src/lib.rs".to_string(), "src/main.rs".to_string()],
        }
    }
}

/// `[[architecture.forbidden]]` — paired glob prohibition.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ForbiddenRule {
    /// Files matching this glob must not import anything matching `to`.
    pub from: String,
    /// Target glob that is forbidden to be imported from `from`.
    pub to: String,
    /// Exceptions: imports matching these globs are allowed despite `to`.
    #[serde(default)]
    pub except: Vec<String>,
    /// Human-readable reason for the rule.
    pub reason: String,
}

/// `[[architecture.pattern]]` — symbol-level restriction.
///
/// Scope is XOR: exactly one of `allowed_in` or `forbidden_in` must be set.
/// At least one matcher must be active.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct SymbolPattern {
    /// Identifier for this rule, used in suppressions and SARIF output.
    pub name: String,

    // ── Scope (XOR) ────────────────────────────────────────────
    /// Whitelist: symbol may only appear in these path globs.
    #[serde(default)]
    pub allowed_in: Option<Vec<String>>,
    /// Blocklist: symbol must not appear in these path globs.
    #[serde(default)]
    pub forbidden_in: Option<Vec<String>>,
    /// Fine-grained exceptions within the scope above.
    #[serde(default)]
    pub except: Vec<String>,

    // ── Matchers (at least one must be active) ────────────────
    /// Match any path reference starting with these prefixes (use/call/attribute/…).
    #[serde(default)]
    pub forbid_path_prefix: Option<Vec<String>>,
    /// Match method calls (`.name(…)` and UFCS `Type::name(…)`).
    #[serde(default)]
    pub forbid_method_call: Option<Vec<String>>,
    /// Match free-function and static-method calls (`name(…)` or `Path::name(…)`).
    #[serde(default)]
    pub forbid_function_call: Option<Vec<String>>,
    /// Match macro invocations (`name!(…)`).
    #[serde(default)]
    pub forbid_macro_call: Option<Vec<String>>,
    /// Match top-level items of specific kinds (e.g. "async_fn", "unsafe_fn",
    /// "inline_cfg_test_module", "top_level_cfg_test_item").
    #[serde(default)]
    pub forbid_item_kind: Option<Vec<String>>,
    /// Match derive annotations (`#[derive(Name)]`).
    #[serde(default)]
    pub forbid_derive: Option<Vec<String>>,
    /// Match glob imports (`use foo::*`) when set to true.
    #[serde(default)]
    pub forbid_glob_import: Option<bool>,
    /// Escape hatch: raw regex applied to AST-aware source (comments/strings masked).
    #[serde(default)]
    pub regex: Option<String>,

    /// Human-readable reason for the rule.
    pub reason: String,
}

/// `[[architecture.trait_contract]]` — structural checks on trait signatures.
///
/// At least one check field must be set.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct TraitContract {
    /// Identifier for this rule, used in suppressions and SARIF output.
    pub name: String,

    /// Glob selecting files whose trait definitions are checked.
    pub scope: String,

    // ── Checks (at least one must be active) ──────────────────
    /// Allowed receiver kinds: "shared_ref" | "mut_ref" | "owned" | "any".
    #[serde(default)]
    pub receiver_may_be: Option<Vec<String>>,
    /// At least one parameter's rendered type must contain this substring.
    #[serde(default)]
    pub required_param_type_contains: Option<String>,
    /// Return type must not contain any of these substrings.
    #[serde(default)]
    pub forbidden_return_type_contains: Option<Vec<String>>,
    /// Error-enum variants must not contain any of these substrings.
    #[serde(default)]
    pub forbidden_error_variant_contains: Option<Vec<String>>,
    /// Explicit list of types treated as "error types" (override naming default).
    #[serde(default)]
    pub error_types: Option<Vec<String>>,
    /// All trait methods in scope must be `async fn`.
    #[serde(default)]
    pub methods_must_be_async: Option<bool>,
    /// Traits in scope must be object-safe (conservative check).
    #[serde(default)]
    pub must_be_object_safe: Option<bool>,
    /// Direct supertrait clause must contain these substrings (non-transitive).
    #[serde(default)]
    pub required_supertraits_contain: Option<Vec<String>>,
}

/// `[architecture.call_parity]` — cross-adapter delegation check.
///
/// Declares a set of peer adapter layers (e.g. `cli`, `mcp`, `rest`) and a
/// shared target layer (e.g. `application`). Two checks run under one rule:
///
/// 1. **No-delegation**: each `pub fn` in an adapter layer must transitively
///    (up to `call_depth` hops) call into the target layer.
/// 2. **Missing-adapter**: each `pub fn` in the target layer must be
///    (transitively) reached from every adapter layer.
///
/// `exclude_targets` is a glob list silencing Check-B for legitimately
/// asymmetric target fns (setup, debug-only endpoints). Fn-level escape
/// via `// qual:allow(architecture)`.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct CallParityConfig {
    /// Layer names that act as peer adapters. Must exist in
    /// `[architecture.layers]`, be disjoint from each other and from `target`.
    pub adapters: Vec<String>,

    /// Target layer name that adapters delegate into. Must exist in
    /// `[architecture.layers]` and not overlap `adapters`.
    pub target: String,

    /// Transitive call-graph depth for both checks. Default 3, range 1..=10.
    #[serde(default = "default_call_depth")]
    pub call_depth: usize,

    /// Glob patterns that silence Check-B (missing-adapter) for matching
    /// target fns.
    ///
    /// Matched against the canonical call target with the leading
    /// `crate::` stripped — i.e. the **module path**, not the layer
    /// name. For `pub fn run()` in `src/app/setup.rs`, the pattern is
    /// `app::setup::run`, independent of whether the owning layer is
    /// called `application`, `app`, or anything else. When the layer
    /// globs happen to mirror the layer name (e.g. layer `application`
    /// mapped to `src/application/**`) the two coincide, but in general
    /// always use the on-disk module path.
    #[serde(default)]
    pub exclude_targets: Vec<String>,

    /// Stage 3 — user-defined transparent wrapper types. These are
    /// peeled during receiver-type resolution just like `Arc`, `Box`,
    /// `Rc`, `Cow`. Typical candidates are framework extractor types:
    /// Axum's `State<T>` / `Extension<T>` / `Json<T>`, Actix's
    /// `Data<T>`, tower's `Router<T>`. Without an entry here,
    /// `fn h(State(db): State<Db>) { db.query() }` leaves `db`
    /// unresolved.
    #[serde(default)]
    pub transparent_wrappers: Vec<String>,

    /// Stage 3 — transparent attribute-macro names. These are
    /// attribute macros whose expansion does not alter the fn body
    /// semantically from a call-graph perspective
    /// (`#[tracing::instrument]`, `#[async_trait]`, `#[tokio::main]`).
    /// The default list covers the most common cases; user entries
    /// extend it. Recorded here for authorial intent and future
    /// extensions — the default syn-based AST walk already treats
    /// attribute macros as transparent, so this config currently
    /// documents rather than changes behaviour.
    #[serde(default)]
    pub transparent_macros: Vec<String>,
}

pub(crate) fn default_call_depth() -> usize {
    3
}

// ── Tests ──────────────────────────────────────────────────────
