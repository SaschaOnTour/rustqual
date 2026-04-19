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

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architecture_config_default_is_disabled() {
        let c = ArchitectureConfig::default();
        assert!(!c.enabled);
        assert!(c.layers.order.is_empty());
        assert_eq!(c.layers.unmatched_behavior, "composition_root");
        assert!(c.external_crates.is_empty());
        assert!(c.forbidden_rules.is_empty());
        assert!(c.patterns.is_empty());
        assert!(c.trait_contracts.is_empty());
    }

    #[test]
    fn test_reexport_points_default_is_lib_and_main() {
        let c = ReexportPointsConfig::default();
        assert_eq!(c.paths, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn test_architecture_enabled_minimal() {
        let toml_str = r#"
            enabled = true
        "#;
        let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
        assert!(c.enabled);
    }

    #[test]
    fn test_architecture_layers_parse() {
        let toml_str = r#"
            [layers]
            order = ["domain", "port", "application", "adapter"]
            unmatched_behavior = "composition_root"

            [layers.domain]
            paths = ["src/domain/**"]

            [layers.application]
            paths = ["src/app/**"]
        "#;
        let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            c.layers.order,
            vec!["domain", "port", "application", "adapter"]
        );
        assert_eq!(c.layers.unmatched_behavior, "composition_root");
        assert_eq!(c.layers.definitions.len(), 2);
        assert_eq!(c.layers.definitions["domain"].paths, vec!["src/domain/**"]);
        assert_eq!(
            c.layers.definitions["application"].paths,
            vec!["src/app/**"]
        );
    }

    #[test]
    fn test_architecture_external_crates_parse() {
        let toml_str = r#"
            [external_crates]
            "pv_core" = "domain"
            "pv_port_*" = "port"
            "pv_adp_*" = "adapter"
        "#;
        let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(c.external_crates.len(), 3);
        assert_eq!(c.external_crates["pv_core"], "domain");
        assert_eq!(c.external_crates["pv_port_*"], "port");
    }

    #[test]
    fn test_architecture_forbidden_parse() {
        let toml_str = r#"
            [[forbidden]]
            from = "src/adapters/a/**"
            to = "src/adapters/b/**"
            reason = "peers are isolated"

            [[forbidden]]
            from = "src/domain/**"
            to = "**"
            except = ["src/domain/**", "src/shared/**"]
            reason = "domain is framework-free"
        "#;
        let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(c.forbidden_rules.len(), 2);
        let first = &c.forbidden_rules[0];
        assert_eq!(first.from, "src/adapters/a/**");
        assert_eq!(first.to, "src/adapters/b/**");
        assert!(first.except.is_empty());
        assert_eq!(first.reason, "peers are isolated");
        let second = &c.forbidden_rules[1];
        assert_eq!(second.except.len(), 2);
    }

    #[test]
    fn test_symbol_pattern_all_matchers_parse() {
        let toml_str = r#"
            [[pattern]]
            name = "everything"
            forbidden_in = ["src/**"]
            forbid_path_prefix = ["tokio::"]
            forbid_method_call = ["unwrap"]
            forbid_function_call = ["Box::new"]
            forbid_macro_call = ["println"]
            forbid_item_kind = ["unsafe_fn"]
            forbid_derive = ["Serialize"]
            forbid_glob_import = true
            regex = 'some\s+pattern'
            reason = "kitchen sink"
        "#;
        let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(c.patterns.len(), 1);
        let p = &c.patterns[0];
        assert_eq!(p.name, "everything");
        assert_eq!(
            p.forbidden_in.as_ref().unwrap(),
            &vec!["src/**".to_string()]
        );
        assert!(p.allowed_in.is_none());
        assert_eq!(
            p.forbid_path_prefix.as_ref().unwrap(),
            &vec!["tokio::".to_string()]
        );
        assert_eq!(
            p.forbid_method_call.as_ref().unwrap(),
            &vec!["unwrap".to_string()]
        );
        assert_eq!(
            p.forbid_function_call.as_ref().unwrap(),
            &vec!["Box::new".to_string()]
        );
        assert_eq!(
            p.forbid_macro_call.as_ref().unwrap(),
            &vec!["println".to_string()]
        );
        assert_eq!(
            p.forbid_item_kind.as_ref().unwrap(),
            &vec!["unsafe_fn".to_string()]
        );
        assert_eq!(
            p.forbid_derive.as_ref().unwrap(),
            &vec!["Serialize".to_string()]
        );
        assert_eq!(p.forbid_glob_import, Some(true));
        assert_eq!(p.regex.as_deref(), Some(r"some\s+pattern"));
    }

    #[test]
    fn test_symbol_pattern_allowed_in_alternative() {
        let toml_str = r#"
            [[pattern]]
            name = "anyhow_only_at_boundary"
            allowed_in = ["src/main.rs", "tests/**"]
            forbid_path_prefix = ["anyhow::"]
            reason = "typed errors outside main"
        "#;
        let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
        let p = &c.patterns[0];
        assert!(p.forbidden_in.is_none());
        assert_eq!(p.allowed_in.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_trait_contract_all_checks_parse() {
        let toml_str = r#"
            [[trait_contract]]
            name = "port_traits"
            scope = "src/ports/**"
            receiver_may_be = ["shared_ref"]
            required_param_type_contains = "CancellationToken"
            forbidden_return_type_contains = ["anyhow::", "Box<dyn"]
            forbidden_error_variant_contains = ["rusqlite::"]
            error_types = ["StoreError"]
            methods_must_be_async = true
            must_be_object_safe = true
            required_supertraits_contain = ["Send", "Sync"]
        "#;
        let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(c.trait_contracts.len(), 1);
        let tc = &c.trait_contracts[0];
        assert_eq!(tc.name, "port_traits");
        assert_eq!(tc.scope, "src/ports/**");
        assert_eq!(
            tc.receiver_may_be.as_ref().unwrap(),
            &vec!["shared_ref".to_string()]
        );
        assert_eq!(
            tc.required_param_type_contains.as_deref(),
            Some("CancellationToken")
        );
        assert_eq!(tc.forbidden_return_type_contains.as_ref().unwrap().len(), 2);
        assert_eq!(
            tc.error_types.as_ref().unwrap(),
            &vec!["StoreError".to_string()]
        );
        assert_eq!(tc.methods_must_be_async, Some(true));
        assert_eq!(tc.must_be_object_safe, Some(true));
        assert_eq!(tc.required_supertraits_contain.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_unknown_field_rejected() {
        // deny_unknown_fields on ArchitectureConfig
        let toml_str = r#"
            enabled = true
            unexpected_field = "oops"
        "#;
        let result: Result<ArchitectureConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err(), "unknown top-level fields must be rejected");
    }

    #[test]
    fn test_symbol_pattern_unknown_field_rejected() {
        let toml_str = r#"
            [[pattern]]
            name = "x"
            forbidden_in = ["src/**"]
            reason = "y"
            bogus_matcher = ["z"]
        "#;
        let result: Result<ArchitectureConfig, _> = toml::from_str(toml_str);
        assert!(
            result.is_err(),
            "unknown fields in pattern must be rejected"
        );
    }

    #[test]
    fn test_forbidden_unknown_field_rejected() {
        let toml_str = r#"
            [[forbidden]]
            from = "a"
            to = "b"
            reason = "c"
            bogus = "d"
        "#;
        let result: Result<ArchitectureConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_reexport_points_custom_paths() {
        let toml_str = r#"
            [reexport_points]
            paths = ["src/lib.rs", "src/main.rs", "src/prelude.rs"]
        "#;
        let c: ArchitectureConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(c.reexport_points.paths.len(), 3);
        assert!(c
            .reexport_points
            .paths
            .iter()
            .any(|p| p == "src/prelude.rs"));
    }
}
