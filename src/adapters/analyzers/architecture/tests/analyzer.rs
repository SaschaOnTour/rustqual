//! Tests for the analyzer's hit→Finding projections + the public
//! `DimensionAnalyzer` surface. Each `Finding` field and rule-id arm is pinned
//! so a deleted field (→ Default) or a swapped rule_id is caught.
use crate::adapters::analyzers::architecture::analyzer::{
    forbidden_hit_to_finding, layer_hit_to_finding,
};
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::domain::{Dimension, Severity};

fn loc(kind: ViolationKind) -> MatchLocation {
    MatchLocation {
        file: "src/a.rs".into(),
        line: 7,
        column: 3,
        kind,
    }
}

#[test]
fn layer_hit_projects_every_finding_field() {
    let f = layer_hit_to_finding(loc(ViolationKind::LayerViolation {
        from_layer: "application".into(),
        to_layer: "domain".into(),
        imported_path: "crate::domain::X".into(),
    }));
    assert_eq!(f.file, "src/a.rs");
    assert_eq!(f.line, 7);
    assert_eq!(f.column, 3);
    assert_eq!(f.dimension, Dimension::Architecture);
    assert_eq!(f.rule_id, "architecture/layer");
    assert_eq!(f.severity, Severity::High);
    assert!(!f.message.is_empty(), "message rendered");
}

#[test]
fn unmatched_layer_hit_uses_unmatched_rule_id() {
    let f = layer_hit_to_finding(loc(ViolationKind::UnmatchedLayer {
        file: "src/a.rs".into(),
    }));
    assert_eq!(
        f.rule_id, "architecture/layer/unmatched",
        "unmatched-layer arm selects its own rule id"
    );
}

#[test]
fn forbidden_hit_projects_every_finding_field() {
    let f = forbidden_hit_to_finding(loc(ViolationKind::ForbiddenEdge {
        reason: "adapters must not import app".into(),
        imported_path: "crate::app::Y".into(),
    }));
    assert_eq!(f.file, "src/a.rs");
    assert_eq!(f.line, 7);
    assert_eq!(f.column, 3);
    assert_eq!(f.dimension, Dimension::Architecture);
    assert_eq!(f.rule_id, "architecture/forbidden");
    assert_eq!(f.severity, Severity::High);
    assert!(
        f.message.contains("adapters must not import app"),
        "forbidden message carries the reason: {}",
        f.message
    );
}

#[test]
fn dimension_name_is_architecture() {
    use crate::adapters::analyzers::architecture::ArchitectureAnalyzer;
    use crate::ports::DimensionAnalyzer;
    assert_eq!(ArchitectureAnalyzer.dimension_name(), "architecture");
}

fn pattern(
    allowed_in: Option<Vec<String>>,
    forbidden_in: Option<Vec<String>>,
) -> crate::adapters::config::architecture::SymbolPattern {
    crate::adapters::config::architecture::SymbolPattern {
        name: "p".into(),
        allowed_in,
        forbidden_in,
        except: vec![],
        forbid_path_prefix: Some(vec!["x::".into()]),
        forbid_method_call: None,
        forbid_function_call: None,
        forbid_macro_call: None,
        forbid_item_kind: None,
        forbid_derive: None,
        forbid_glob_import: None,
        allow_prelude_glob: true,
        regex: None,
        reason: String::new(),
    }
}

#[test]
fn compile_pattern_scope_requires_exactly_one_of_allowed_forbidden() {
    use crate::adapters::analyzers::architecture::analyzer::compile_pattern_scope;
    // allowed_in only → AllowedIn scope (Some); forbidden_in only → ForbiddenIn
    // (Some). Pins both match arms + the `build_globset` success path.
    assert!(
        compile_pattern_scope(&pattern(Some(vec!["src/**".into()]), None)).is_some(),
        "allowed_in-only compiles"
    );
    assert!(
        compile_pattern_scope(&pattern(None, Some(vec!["src/**".into()]))).is_some(),
        "forbidden_in-only compiles"
    );
    // Neither / both → None (the XOR `_ => None`).
    assert!(
        compile_pattern_scope(&pattern(None, None)).is_none(),
        "neither → None"
    );
    assert!(
        compile_pattern_scope(&pattern(Some(vec!["a".into()]), Some(vec!["b".into()]))).is_none(),
        "both → None"
    );
    // An invalid glob fails compilation (`build_globset` → None).
    assert!(
        compile_pattern_scope(&pattern(Some(vec!["[".into()]), None)).is_none(),
        "invalid glob → None"
    );
}

#[test]
fn pattern_scope_accepts_implements_forbidden_allowed_and_except() {
    use crate::adapters::analyzers::architecture::analyzer::compile_pattern_scope;
    // The scope-COMPILE test above only checks Some/None. This pins the actual
    // `accepts()` decision — the spec's "where it fires" mechanic — which was
    // otherwise untested (the `pattern()` helper always leaves `except` empty).

    // ForbiddenIn: fires INSIDE the forbidden paths, exempt outside.
    let forbidden =
        compile_pattern_scope(&pattern(None, Some(vec!["src/domain/**".into()]))).unwrap();
    assert!(
        forbidden.accepts("src/domain/bad.rs"),
        "ForbiddenIn fires inside its paths"
    );
    assert!(
        !forbidden.accepts("src/app/ok.rs"),
        "ForbiddenIn exempt outside its paths"
    );

    // AllowedIn: the INVERSE — fires EVERYWHERE EXCEPT the allowed paths (the
    // spec's "println! only in cli" / "syn:: only in adapters" semantics).
    let allowed = compile_pattern_scope(&pattern(Some(vec!["src/cli/**".into()]), None)).unwrap();
    assert!(
        !allowed.accepts("src/cli/main.rs"),
        "AllowedIn exempt inside its paths"
    );
    assert!(
        allowed.accepts("src/domain/bad.rs"),
        "AllowedIn fires outside its paths"
    );

    // `except` overrides both: a path in except is never matched, even inside
    // forbidden_in.
    let mut p = pattern(None, Some(vec!["src/domain/**".into()]));
    p.except = vec!["src/domain/generated/**".into()];
    let scoped = compile_pattern_scope(&p).unwrap();
    assert!(
        scoped.accepts("src/domain/bad.rs"),
        "still fires in forbidden_in"
    );
    assert!(
        !scoped.accepts("src/domain/generated/gen.rs"),
        "except exempts even inside forbidden_in"
    );
}

fn two_layer_config() -> crate::config::ArchitectureConfig {
    use crate::config::architecture::{
        ArchitectureLayersConfig, LayerPathsConfig, ReexportPointsConfig,
    };
    use std::collections::HashMap;
    let mut definitions = HashMap::new();
    definitions.insert(
        "domain".into(),
        LayerPathsConfig {
            paths: vec!["src/domain/**".into()],
        },
    );
    definitions.insert(
        "adapter".into(),
        LayerPathsConfig {
            paths: vec!["src/adapters/**".into()],
        },
    );
    let mut cfg = crate::config::ArchitectureConfig {
        enabled: true,
        ..Default::default()
    };
    cfg.layers = ArchitectureLayersConfig {
        order: vec!["domain".into(), "adapter".into()],
        unmatched_behavior: "composition_root".into(),
        definitions,
    };
    cfg.reexport_points = ReexportPointsConfig { paths: vec![] };
    cfg
}

#[test]
fn analyze_emits_findings_for_layer_violation_only_when_enabled() {
    use crate::adapters::analyzers::architecture::ArchitectureAnalyzer;
    use crate::ports::{AnalysisContext, DimensionAnalyzer, ParsedFile};
    let src = "use crate::adapters::X;";
    let files = vec![ParsedFile {
        path: "src/domain/bad.rs".into(),
        content: src.into(),
        ast: syn::parse_file(src).unwrap(),
    }];
    // Enabled: a domain file importing an adapter is an inner→outer layer
    // violation → at least one finding (pins `analyze -> vec![]` and the
    // `!arch.enabled` guard).
    let mut config = crate::config::Config::default();
    config.architecture = two_layer_config();
    let ctx = AnalysisContext {
        files: &files,
        config: &config,
    };
    assert!(
        !ArchitectureAnalyzer.analyze(&ctx).is_empty(),
        "enabled → finding"
    );
    // Disabled: same input → no findings.
    config.architecture.enabled = false;
    let ctx = AnalysisContext {
        files: &files,
        config: &config,
    };
    assert!(
        ArchitectureAnalyzer.analyze(&ctx).is_empty(),
        "disabled → empty"
    );
}

#[test]
fn glob_policy_prelude_exemption_is_config_gated() {
    // `forbid_glob_import` reports ALL globs at the matcher level; the analyzer
    // policy then forgives `*::prelude::*` iff `allow_prelude_glob` (default
    // true). The plain `crate::guts::*` glob always fires.
    use crate::adapters::analyzers::architecture::analyzer::run_pattern_matchers;
    let src = "use dioxus::prelude::*;\nuse crate::guts::*;\n";
    let file = crate::ports::ParsedFile {
        path: "src/a.rs".into(),
        content: src.into(),
        ast: syn::parse_file(src).expect("parse fixture"),
    };
    let mut pat = pattern(None, None);
    pat.forbid_path_prefix = None;
    pat.forbid_glob_import = Some(true);

    // Default (true): prelude glob (line 1) forgiven, only the plain glob (line 2) fires.
    pat.allow_prelude_glob = true;
    let exempt = run_pattern_matchers(&file, &pat);
    assert_eq!(exempt.len(), 1, "prelude must be exempt, got {exempt:?}");
    assert_eq!(
        exempt[0].line, 2,
        "the surviving finding is the non-prelude glob"
    );

    // Opt-out (false): every glob fires, including the prelude one.
    pat.allow_prelude_glob = false;
    let strict = run_pattern_matchers(&file, &pat);
    assert_eq!(
        strict.len(),
        2,
        "allow_prelude_glob = false forbids even prelude globs, got {strict:?}"
    );
}
