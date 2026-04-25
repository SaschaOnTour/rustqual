use crate::adapters::analyzers::architecture::compiled::*;
use crate::adapters::analyzers::architecture::forbidden_rule::CompiledForbiddenRule;
use crate::adapters::analyzers::architecture::layer_rule::{LayerDefinitions, UnmatchedBehavior};
use crate::config::architecture::{
    ArchitectureLayersConfig, CallParityConfig, ForbiddenRule, LayerPathsConfig,
    ReexportPointsConfig,
};
use crate::config::ArchitectureConfig;
use globset::{Glob, GlobMatcher, GlobSet, GlobSetBuilder};
use std::collections::HashMap;

fn layer_def(path: &str) -> LayerPathsConfig {
    LayerPathsConfig {
        paths: vec![path.to_string()],
    }
}

fn cfg_with_layers(layers: &[(&str, &str)]) -> ArchitectureConfig {
    let order = layers.iter().map(|(n, _)| n.to_string()).collect();
    let definitions = layers
        .iter()
        .map(|(n, p)| (n.to_string(), layer_def(p)))
        .collect();
    ArchitectureConfig {
        enabled: true,
        layers: ArchitectureLayersConfig {
            order,
            unmatched_behavior: "composition_root".to_string(),
            definitions,
        },
        reexport_points: ReexportPointsConfig {
            paths: vec!["src/lib.rs".to_string()],
        },
        external_crates: HashMap::new(),
        forbidden_rules: Vec::new(),
        patterns: Vec::new(),
        trait_contracts: Vec::new(),
        call_parity: None,
    }
}

fn minimal_cfg() -> ArchitectureConfig {
    cfg_with_layers(&[("domain", "src/domain/**"), ("adapter", "src/adapters/**")])
}

/// 3-layer fixture for call_parity tests: application + cli + mcp.
fn call_parity_cfg() -> ArchitectureConfig {
    cfg_with_layers(&[
        ("application", "src/application/**"),
        ("cli", "src/cli/**"),
        ("mcp", "src/mcp/**"),
    ])
}

#[test]
fn compiles_minimal_config() {
    let c = compile_architecture(&minimal_cfg()).expect("compile");
    assert_eq!(c.layers.rank_of("domain"), Some(0));
    assert_eq!(c.layers.rank_of("adapter"), Some(1));
    assert!(c.reexport_points.is_match("src/lib.rs"));
}

#[test]
fn rejects_order_with_missing_definition() {
    let mut cfg = minimal_cfg();
    cfg.layers.order.push("ghost".to_string());
    let err = compile_architecture(&cfg).unwrap_err();
    assert!(err.contains("ghost"), "err = {err}");
}

#[test]
fn rejects_invalid_unmatched_behavior() {
    let mut cfg = minimal_cfg();
    cfg.layers.unmatched_behavior = "bogus".to_string();
    let err = compile_architecture(&cfg).unwrap_err();
    assert!(err.contains("bogus"), "err = {err}");
}

#[test]
fn external_crates_split_by_glob_chars() {
    let mut cfg = minimal_cfg();
    cfg.external_crates
        .insert("tokio".to_string(), "adapter".to_string());
    cfg.external_crates
        .insert("adp_*".to_string(), "adapter".to_string());
    let c = compile_architecture(&cfg).expect("compile");
    assert_eq!(c.external_exact.get("tokio"), Some(&"adapter".to_string()));
    assert_eq!(c.external_glob.len(), 1);
}

#[test]
fn forbidden_rules_compile() {
    let mut cfg = minimal_cfg();
    cfg.forbidden_rules.push(ForbiddenRule {
        from: "src/domain/**".to_string(),
        to: "src/adapters/**".to_string(),
        except: vec!["src/adapters/shared/**".to_string()],
        reason: "isolated".to_string(),
    });
    let c = compile_architecture(&cfg).expect("compile");
    assert_eq!(c.forbidden.len(), 1);
    assert!(c.forbidden[0].from.is_match("src/domain/foo.rs"));
    assert_eq!(c.forbidden[0].reason, "isolated");
}

// ── Call-Parity compile/validation (Task 0) ───────────────────

fn minimal_call_parity() -> CallParityConfig {
    CallParityConfig {
        adapters: vec!["cli".to_string(), "mcp".to_string()],
        target: "application".to_string(),
        call_depth: 3,
        exclude_targets: Vec::new(),
        transparent_wrappers: Vec::new(),
        transparent_macros: Vec::new(),
    }
}

#[test]
fn compile_call_parity_none_yields_none() {
    let c = compile_architecture(&call_parity_cfg()).expect("compile");
    assert!(c.call_parity.is_none());
}

#[test]
fn compile_call_parity_links_layer_refs() {
    let mut cfg = call_parity_cfg();
    cfg.call_parity = Some(minimal_call_parity());
    let c = compile_architecture(&cfg).expect("compile");
    let cp = c.call_parity.expect("call_parity compiled");
    assert_eq!(cp.adapters, vec!["cli".to_string(), "mcp".to_string()]);
    assert_eq!(cp.target, "application");
    assert_eq!(cp.call_depth, 3);
    assert!(cp.exclude_targets.is_empty());
}

#[test]
fn compile_call_parity_rejects_unknown_adapter_layer() {
    let mut cfg = call_parity_cfg();
    let mut cp = minimal_call_parity();
    cp.adapters.push("ghost".to_string());
    cfg.call_parity = Some(cp);
    let err = compile_architecture(&cfg).unwrap_err();
    assert!(err.contains("ghost"), "err = {err}");
    assert!(err.to_lowercase().contains("layer"), "err = {err}");
}

#[test]
fn compile_call_parity_rejects_unknown_target_layer() {
    let mut cfg = call_parity_cfg();
    let mut cp = minimal_call_parity();
    cp.target = "ghost".to_string();
    cfg.call_parity = Some(cp);
    let err = compile_architecture(&cfg).unwrap_err();
    assert!(err.contains("ghost"), "err = {err}");
}

#[test]
fn compile_call_parity_rejects_empty_adapters() {
    let mut cfg = call_parity_cfg();
    let mut cp = minimal_call_parity();
    cp.adapters.clear();
    cfg.call_parity = Some(cp);
    let err = compile_architecture(&cfg).unwrap_err();
    assert!(
        err.to_lowercase().contains("non-empty") || err.to_lowercase().contains("empty"),
        "err = {err}"
    );
}

#[test]
fn compile_call_parity_rejects_target_in_adapters() {
    let mut cfg = call_parity_cfg();
    let mut cp = minimal_call_parity();
    cp.adapters.push("application".to_string()); // same as target
    cfg.call_parity = Some(cp);
    let err = compile_architecture(&cfg).unwrap_err();
    assert!(err.to_lowercase().contains("target"), "err = {err}");
}

#[test]
fn compile_call_parity_rejects_duplicate_adapters() {
    let mut cfg = call_parity_cfg();
    let mut cp = minimal_call_parity();
    cp.adapters = vec!["cli".to_string(), "cli".to_string()];
    cfg.call_parity = Some(cp);
    let err = compile_architecture(&cfg).unwrap_err();
    assert!(
        err.to_lowercase().contains("duplicate") || err.to_lowercase().contains("disjoint"),
        "err = {err}"
    );
}

#[test]
fn compile_call_parity_rejects_call_depth_zero() {
    let mut cfg = call_parity_cfg();
    let mut cp = minimal_call_parity();
    cp.call_depth = 0;
    cfg.call_parity = Some(cp);
    let err = compile_architecture(&cfg).unwrap_err();
    assert!(err.to_lowercase().contains("call_depth"), "err = {err}");
}

#[test]
fn compile_call_parity_rejects_call_depth_too_large() {
    let mut cfg = call_parity_cfg();
    let mut cp = minimal_call_parity();
    cp.call_depth = 11;
    cfg.call_parity = Some(cp);
    let err = compile_architecture(&cfg).unwrap_err();
    assert!(err.to_lowercase().contains("call_depth"), "err = {err}");
}

#[test]
fn compile_call_parity_accepts_exclude_targets() {
    let mut cfg = call_parity_cfg();
    let mut cp = minimal_call_parity();
    cp.exclude_targets = vec![
        "application::setup::*".to_string(),
        "application::mcp::*".to_string(),
    ];
    cfg.call_parity = Some(cp);
    let c = compile_architecture(&cfg).expect("compile");
    let compiled_cp = c.call_parity.unwrap();
    assert!(compiled_cp
        .exclude_targets
        .is_match("application::setup::run"));
    assert!(compiled_cp
        .exclude_targets
        .is_match("application::mcp::dispatch"));
    assert!(!compiled_cp
        .exclude_targets
        .is_match("application::stats::get"));
}

#[test]
fn compile_call_parity_rejects_invalid_exclude_glob() {
    let mut cfg = call_parity_cfg();
    let mut cp = minimal_call_parity();
    cp.exclude_targets = vec!["[".to_string()]; // malformed glob
    cfg.call_parity = Some(cp);
    let err = compile_architecture(&cfg).unwrap_err();
    assert!(
        err.to_lowercase().contains("exclude_targets"),
        "err = {err}"
    );
}

#[test]
fn compile_call_parity_normalises_transparent_wrappers() {
    // Resolver lookups are keyed on the bare type ident, so config
    // values may include path prefixes (`axum::extract::State`) and
    // generic suffixes (`State<T>`); both must reduce to `State`.
    let mut cfg = call_parity_cfg();
    let mut cp = minimal_call_parity();
    cp.transparent_wrappers = vec![
        "  State  ".to_string(),
        "axum::extract::Extension".to_string(),
        "Json<T>".to_string(),
        "actix_web::web::Data<DbPool>".to_string(),
        // Path-qualified generic arg: the `::` lives inside the
        // generic, so naive last-`::`-split picks `Db>` instead of
        // `State`. Must strip `<…>` before splitting.
        "axum::extract::State<crate::app::Db>".to_string(),
    ];
    cfg.call_parity = Some(cp);
    let c = compile_architecture(&cfg).expect("compile");
    let wrappers = c.call_parity.unwrap().transparent_wrappers;
    assert!(wrappers.contains("State"), "wrappers = {wrappers:?}");
    assert!(wrappers.contains("Extension"), "wrappers = {wrappers:?}");
    assert!(wrappers.contains("Json"), "wrappers = {wrappers:?}");
    assert!(wrappers.contains("Data"), "wrappers = {wrappers:?}");
    // No leftover entries with `<` or whitespace.
    for w in &wrappers {
        assert!(
            !w.contains('<') && w == w.trim(),
            "wrapper key not normalised: {w:?}"
        );
    }
}

// ── LayerDefinitions::layer_of_crate_path (Task 2) ──────────────

fn layers_for_crate_path() -> LayerDefinitions {
    // Four layers covering typical adapter / application / infrastructure
    // boundaries. Rank ordering: domain=0, application=1, cli=2, mcp=3.
    let defs = vec![
        ("domain".to_string(), globset_for(&["src/domain/**"])),
        (
            "application".to_string(),
            globset_for(&["src/application/**"]),
        ),
        ("cli".to_string(), globset_for(&["src/cli/**"])),
        ("mcp".to_string(), globset_for(&["src/mcp/**"])),
    ];
    LayerDefinitions::new(
        vec![
            "domain".to_string(),
            "application".to_string(),
            "cli".to_string(),
            "mcp".to_string(),
        ],
        defs,
    )
}

fn globset_for(patterns: &[&str]) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).unwrap());
    }
    b.build().unwrap()
}

#[test]
fn test_layer_of_crate_path_matches_inner_layer() {
    let layers = layers_for_crate_path();
    // `crate::application::stats::get_stats` should resolve via file
    // candidate `src/application/stats.rs` or `src/application/stats/mod.rs`.
    assert_eq!(
        layers.layer_of_crate_path("crate::application::stats::get_stats"),
        Some("application"),
        "application target must be recognised"
    );
    assert_eq!(
        layers.layer_of_crate_path("crate::cli::handlers::cmd_stats"),
        Some("cli"),
    );
    assert_eq!(
        layers.layer_of_crate_path("crate::mcp::handlers::handle_stats"),
        Some("mcp"),
    );
}

#[test]
fn test_layer_of_crate_path_returns_none_for_method_and_bare() {
    let layers = layers_for_crate_path();
    assert_eq!(layers.layer_of_crate_path("<method>:search"), None);
    assert_eq!(layers.layer_of_crate_path("<bare>:Box::new"), None);
    assert_eq!(layers.layer_of_crate_path("<bare>:foo"), None);
}

#[test]
fn test_layer_of_crate_path_returns_none_for_unmapped() {
    let layers = layers_for_crate_path();
    // No layer covers `src/unmatched/**`.
    assert_eq!(layers.layer_of_crate_path("crate::unmatched::helper"), None);
}

#[test]
fn test_layer_of_crate_path_handles_mod_rs_and_bare_rs() {
    // Synthesising candidate paths must try both `src/x/y.rs` and
    // `src/x/y/mod.rs` — otherwise a layer defined as `src/cli/**` would
    // miss `crate::cli` (which points at `src/cli/mod.rs`).
    let layers = layers_for_crate_path();
    assert_eq!(
        layers.layer_of_crate_path("crate::cli::CliSession"),
        Some("cli"),
    );
    assert_eq!(
        layers.layer_of_crate_path("crate::cli"),
        Some("cli"),
        "cli root module itself must resolve to cli layer"
    );
}

#[test]
fn test_layer_of_crate_path_picks_first_match() {
    // Two layers both matching — order (rank) decides; first in the
    // `definitions` list wins. Building in [domain=src/**, cli=src/cli/**]
    // order forces `src/cli/handler.rs` to resolve to domain (greedy).
    let defs = vec![
        ("wide".to_string(), globset_for(&["src/**"])),
        ("narrow".to_string(), globset_for(&["src/cli/**"])),
    ];
    let layers = LayerDefinitions::new(vec!["wide".to_string(), "narrow".to_string()], defs);
    assert_eq!(
        layers.layer_of_crate_path("crate::cli::handler::foo"),
        Some("wide"),
        "first matching layer wins, same semantics as layer_for_file"
    );
}

#[test]
fn test_layer_of_crate_path_rejects_non_crate_prefix() {
    let layers = layers_for_crate_path();
    // Non-crate-rooted input (should never happen — canonical targets
    // always start with `crate::` when they're resolvable).
    assert_eq!(layers.layer_of_crate_path("std::fs::read"), None);
    assert_eq!(layers.layer_of_crate_path(""), None);
}

#[test]
fn test_layer_of_crate_path_resolves_crate_root_items() {
    // `crate::run` can target a fn declared directly in `src/lib.rs` or
    // `src/main.rs`. Without a crate-root fallback, Check A would
    // false-positive "no delegation" when an adapter calls such a fn.
    let defs = vec![
        (
            "root".to_string(),
            globset_for(&["src/lib.rs", "src/main.rs"]),
        ),
        ("adapter".to_string(), globset_for(&["src/cli/**"])),
    ];
    let layers = LayerDefinitions::new(vec!["root".to_string(), "adapter".to_string()], defs);
    assert_eq!(
        layers.layer_of_crate_path("crate::run"),
        Some("root"),
        "crate-root single-segment path must resolve to lib/main's layer"
    );
}
