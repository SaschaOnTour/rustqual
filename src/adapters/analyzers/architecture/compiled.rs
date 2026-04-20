//! Compile the raw `[architecture]` config into runtime structures.
//!
//! The TOML config is parse-only; rule evaluation needs pre-compiled globs,
//! per-rule matchers, and rank lookups. `compile_architecture` produces one
//! `CompiledArchitecture` that the layer rule, forbidden rule, and the
//! `--explain` diagnostic all share.

#![cfg_attr(test, allow(dead_code))]

use crate::adapters::analyzers::architecture::forbidden_rule::CompiledForbiddenRule;
use crate::adapters::analyzers::architecture::layer_rule::{LayerDefinitions, UnmatchedBehavior};
use crate::adapters::analyzers::architecture::trait_contract_rule::CompiledTraitContract;
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
    pub trait_contracts: Vec<CompiledTraitContract>,
}

/// Compile the raw config into `CompiledArchitecture`.
/// Integration: delegates per-field compilation to sub-operations.
pub fn compile_architecture(cfg: &ArchitectureConfig) -> Result<CompiledArchitecture, String> {
    let layers = compile_layers(cfg)?;
    let reexport_points = compile_reexport_points(cfg)?;
    let unmatched_behavior = parse_unmatched_behavior(&cfg.layers.unmatched_behavior)?;
    let (external_exact, external_glob) = compile_external_crates(&cfg.external_crates)?;
    let forbidden = compile_forbidden_rules(&cfg.forbidden_rules)?;
    let trait_contracts = compile_trait_contracts(&cfg.trait_contracts)?;
    Ok(CompiledArchitecture {
        layers,
        reexport_points,
        unmatched_behavior,
        external_exact,
        external_glob,
        forbidden,
        trait_contracts,
    })
}

/// Compile `[[architecture.trait_contract]]` entries into runtime rules.
/// Operation: per-entry glob compilation + field copy.
fn compile_trait_contracts(
    raw: &[crate::config::architecture::TraitContract],
) -> Result<Vec<CompiledTraitContract>, String> {
    raw.iter()
        .map(|tc| {
            let scope = Glob::new(&tc.scope)
                .map_err(|e| format!("trait_contract.scope \"{}\": {e}", tc.scope))?;
            let mut b = GlobSetBuilder::new();
            b.add(scope);
            let scope = b
                .build()
                .map_err(|e| format!("trait_contract.scope: {e}"))?;
            Ok(CompiledTraitContract {
                name: tc.name.clone(),
                scope,
                receiver_may_be: tc.receiver_may_be.clone(),
                required_param_type_contains: tc.required_param_type_contains.clone(),
                forbidden_return_type_contains: tc
                    .forbidden_return_type_contains
                    .clone()
                    .unwrap_or_default(),
                forbidden_error_variant_contains: tc
                    .forbidden_error_variant_contains
                    .clone()
                    .unwrap_or_default(),
                error_types: tc.error_types.clone().unwrap_or_default(),
                methods_must_be_async: tc.methods_must_be_async,
                must_be_object_safe: tc.must_be_object_safe,
                required_supertraits_contain: tc
                    .required_supertraits_contain
                    .clone()
                    .unwrap_or_default(),
            })
        })
        .collect()
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

/// Return `true` iff `key` is a literal crate name (matches the
/// code-level identifier, i.e. `[A-Za-z0-9_]+`). Any other character
/// (including `*`, `?`, `[`, `{`) means the key is a glob pattern and
/// must be compiled via `globset`. Hyphen is intentionally excluded:
/// Rust imports use the underscore form (`tokio_util`, not
/// `tokio-util`), so a hyphenated exact key could never match at
/// runtime; users who want to match a hyphenated Cargo package name
/// must supply the underscore form or a glob.
/// Operation: pure predicate over the character set.
fn is_exact_crate_key(key: &str) -> bool {
    !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Compile the `external_crates` map into an exact map and a glob list.
/// Exact map = literal crate-name keys (`[A-Za-z0-9_]+`); glob list = any
/// key containing glob meta (`*`, `?`, `[...]`, `{a,b}`, etc.) or a
/// hyphen (Rust-code import form uses underscores).
/// Operation: per-entry classification + compilation.
fn compile_external_crates(raw: &HashMap<String, String>) -> Result<ExternalCrates, String> {
    let mut exact = HashMap::new();
    let mut globs = Vec::new();
    for (key, layer) in raw {
        if is_exact_crate_key(key) {
            exact.insert(key.clone(), layer.clone());
        } else {
            let matcher = Glob::new(key)
                .map_err(|e| format!("external_crates \"{key}\" glob error: {e}"))?
                .compile_matcher();
            globs.push((matcher, layer.clone()));
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
