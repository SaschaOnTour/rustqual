//! Compile the raw `[architecture]` config into runtime structures.
//!
//! The TOML config is parse-only; rule evaluation needs pre-compiled globs,
//! per-rule matchers, and rank lookups. `compile_architecture` produces one
//! `CompiledArchitecture` that the layer rule, forbidden rule, and the
//! `--explain` diagnostic all share.

#![allow(dead_code)]

use crate::adapters::analyzers::architecture::forbidden_rule::CompiledForbiddenRule;
use crate::adapters::analyzers::architecture::layer_rule::{LayerDefinitions, UnmatchedBehavior};
use crate::config::ArchitectureConfig;
use globset::{Glob, GlobMatcher, GlobSet, GlobSetBuilder};
use std::collections::HashMap;

type ExternalCrates = (HashMap<String, String>, Vec<(GlobMatcher, String)>);

/// Runtime-ready architecture configuration.
#[derive(Debug)]
pub struct CompiledArchitecture {
    pub layers: LayerDefinitions,
    pub reexport_points: GlobSet,
    pub unmatched_behavior: UnmatchedBehavior,
    pub external_exact: HashMap<String, String>,
    pub external_glob: Vec<(GlobMatcher, String)>,
    pub forbidden: Vec<CompiledForbiddenRule>,
}

/// Compile the raw config into `CompiledArchitecture`.
/// Integration: delegates per-field compilation to sub-operations.
pub fn compile_architecture(cfg: &ArchitectureConfig) -> Result<CompiledArchitecture, String> {
    let layers = compile_layers(cfg)?;
    let reexport_points = compile_reexport_points(cfg)?;
    let unmatched_behavior = parse_unmatched_behavior(&cfg.layers.unmatched_behavior)?;
    let (external_exact, external_glob) = compile_external_crates(&cfg.external_crates)?;
    let forbidden = compile_forbidden_rules(&cfg.forbidden_rules)?;
    Ok(CompiledArchitecture {
        layers,
        reexport_points,
        unmatched_behavior,
        external_exact,
        external_glob,
        forbidden,
    })
}

/// Compile `[architecture.layers]` into a `LayerDefinitions`.
/// Operation: per-layer glob-set compilation.
fn compile_layers(cfg: &ArchitectureConfig) -> Result<LayerDefinitions, String> {
    let order = cfg.layers.order.clone();
    let mut definitions = Vec::with_capacity(order.len());
    for name in &order {
        let Some(paths_cfg) = cfg.layers.definitions.get(name) else {
            return Err(format!("layer \"{name}\" listed in order but has no paths"));
        };
        let gs = build_globset(&paths_cfg.paths)
            .map_err(|e| format!("layer \"{name}\" glob error: {e}"))?;
        definitions.push((name.clone(), gs));
    }
    Ok(LayerDefinitions::new(order, definitions))
}

/// Compile `[architecture.reexport_points]` into a GlobSet.
/// Operation: single glob-set build.
fn compile_reexport_points(cfg: &ArchitectureConfig) -> Result<GlobSet, String> {
    build_globset(&cfg.reexport_points.paths).map_err(|e| format!("reexport_points: {e}"))
}

/// Parse the `unmatched_behavior` string.
/// Operation: string-match dispatch.
fn parse_unmatched_behavior(raw: &str) -> Result<UnmatchedBehavior, String> {
    match raw {
        "composition_root" => Ok(UnmatchedBehavior::CompositionRoot),
        "strict_error" => Ok(UnmatchedBehavior::StrictError),
        other => Err(format!(
            "unmatched_behavior must be \"composition_root\" or \"strict_error\", got {other:?}"
        )),
    }
}

/// Compile the `external_crates` map into an exact map and a glob list.
/// Exact map = keys without glob meta-characters; glob list = keys with `*` or `?`.
/// Operation: per-entry classification + compilation.
fn compile_external_crates(raw: &HashMap<String, String>) -> Result<ExternalCrates, String> {
    let mut exact = HashMap::new();
    let mut globs = Vec::new();
    for (key, layer) in raw {
        if key.contains('*') || key.contains('?') {
            let matcher = Glob::new(key)
                .map_err(|e| format!("external_crates \"{key}\" glob error: {e}"))?
                .compile_matcher();
            globs.push((matcher, layer.clone()));
        } else {
            exact.insert(key.clone(), layer.clone());
        }
    }
    Ok((exact, globs))
}

/// Compile `[[architecture.forbidden]]` entries into `CompiledForbiddenRule`s.
/// Operation: per-entry glob compilation.
fn compile_forbidden_rules(
    raw: &[crate::config::architecture::ForbiddenRule],
) -> Result<Vec<CompiledForbiddenRule>, String> {
    let mut out = Vec::with_capacity(raw.len());
    for rule in raw {
        let from = Glob::new(&rule.from)
            .map_err(|e| format!("forbidden.from \"{}\": {e}", rule.from))?
            .compile_matcher();
        let to = Glob::new(&rule.to)
            .map_err(|e| format!("forbidden.to \"{}\": {e}", rule.to))?
            .compile_matcher();
        let except = build_globset(&rule.except).map_err(|e| format!("forbidden.except: {e}"))?;
        out.push(CompiledForbiddenRule {
            from,
            to,
            except,
            reason: rule.reason.clone(),
        });
    }
    Ok(out)
}

/// Build a `GlobSet` from a list of patterns, returning the first compile error.
/// Operation: iterative glob addition with failure short-circuit.
fn build_globset(patterns: &[String]) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    for pat in patterns {
        let g = Glob::new(pat).map_err(|e| format!("glob \"{pat}\": {e}"))?;
        builder.add(g);
    }
    builder.build().map_err(|e| format!("globset build: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::architecture::{
        ArchitectureLayersConfig, ForbiddenRule, LayerPathsConfig, ReexportPointsConfig,
    };

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
}
