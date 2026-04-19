use crate::adapters::analyzers::architecture::compiled::*;
use crate::adapters::analyzers::architecture::forbidden_rule::CompiledForbiddenRule;
use crate::adapters::analyzers::architecture::layer_rule::{LayerDefinitions, UnmatchedBehavior};
use crate::config::architecture::{
    ArchitectureLayersConfig, ForbiddenRule, LayerPathsConfig, ReexportPointsConfig,
};
use crate::config::ArchitectureConfig;
use globset::{Glob, GlobMatcher, GlobSet, GlobSetBuilder};
use std::collections::HashMap;

fn minimal_cfg() -> ArchitectureConfig {
    let mut defs = HashMap::new();
    defs.insert(
        "domain".to_string(),
        LayerPathsConfig {
            paths: vec!["src/domain/**".to_string()],
        },
    );
    defs.insert(
        "adapter".to_string(),
        LayerPathsConfig {
            paths: vec!["src/adapters/**".to_string()],
        },
    );
    ArchitectureConfig {
        enabled: true,
        layers: ArchitectureLayersConfig {
            order: vec!["domain".to_string(), "adapter".to_string()],
            unmatched_behavior: "composition_root".to_string(),
            definitions: defs,
        },
        reexport_points: ReexportPointsConfig {
            paths: vec!["src/lib.rs".to_string()],
        },
        external_crates: HashMap::new(),
        forbidden_rules: Vec::new(),
        patterns: Vec::new(),
        trait_contracts: Vec::new(),
    }
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
