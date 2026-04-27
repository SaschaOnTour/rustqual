//! Tests for Check C — multi-touchpoint detection.
//!
//! Check C fires when an adapter pub-fn has more than one touchpoint
//! in the target layer — i.e. it orchestrates multiple application
//! calls itself instead of wrapping them inside a single application
//! method. Configurable severity via `single_touchpoint`.

use super::support::{build_workspace, empty_cfg_test, globset, run_check_c, three_layer};
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::config::architecture::SingleTouchpointMode;
use std::collections::HashSet;

fn make_config(mode: SingleTouchpointMode) -> CompiledCallParity {
    CompiledCallParity {
        adapters: vec!["cli".to_string(), "mcp".to_string()],
        target: "application".to_string(),
        call_depth: 3,
        exclude_targets: globset(&[]),
        transparent_wrappers: HashSet::new(),
        transparent_macros: HashSet::new(),
        single_touchpoint: mode,
    }
}

fn extract_c(findings: &[MatchLocation]) -> Vec<(String, Vec<String>)> {
    findings
        .iter()
        .filter_map(|f| match &f.kind {
            ViolationKind::CallParityMultiTouchpoint {
                fn_name,
                touchpoints,
                ..
            } => Some((fn_name.clone(), touchpoints.clone())),
            _ => None,
        })
        .collect()
}

// ── Single touchpoint → silent ────────────────────────────────

#[test]
fn check_c_single_touchpoint_silent() {
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            "#,
        ),
    ]);
    let cp = make_config(SingleTouchpointMode::Warn);
    let findings = run_check_c(&ws, &three_layer(), &cp, &empty_cfg_test());
    assert!(
        extract_c(&findings).is_empty(),
        "single-touchpoint handler should be silent, got {findings:?}"
    );
}

// ── Two touchpoints, default severity warn ────────────────────

#[test]
fn check_c_two_touchpoints_warn_default() {
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub fn foo() {}
            pub fn bar() {}
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::{foo, bar};
            pub fn cmd_orchestrate() { foo(); bar(); }
            "#,
        ),
    ]);
    let cp = make_config(SingleTouchpointMode::Warn);
    let findings = run_check_c(&ws, &three_layer(), &cp, &empty_cfg_test());
    let pairs = extract_c(&findings);
    assert_eq!(pairs.len(), 1, "got {findings:?}");
    let mut tps = pairs[0].1.clone();
    tps.sort();
    assert_eq!(
        tps,
        vec![
            "crate::application::session::bar".to_string(),
            "crate::application::session::foo".to_string(),
        ]
    );
}

// ── Branch with two distinct targets → C fires ────────────────

#[test]
fn check_c_branch_two_targets() {
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub fn foo() {}
            pub fn bar() {}
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::{foo, bar};
            pub fn cmd_branch(cond: bool) {
                if cond { foo(); } else { bar(); }
            }
            "#,
        ),
    ]);
    let cp = make_config(SingleTouchpointMode::Warn);
    let findings = run_check_c(&ws, &three_layer(), &cp, &empty_cfg_test());
    let pairs = extract_c(&findings);
    assert_eq!(pairs.len(), 1, "got {findings:?}");
    let mut tps = pairs[0].1.clone();
    tps.sort();
    assert_eq!(tps.len(), 2);
}

// ── Severity: error mode → silently still emits but consumer can filter ──

#[test]
fn check_c_severity_off_skips_check() {
    // single_touchpoint = Off must produce zero findings even with a
    // multi-touchpoint handler.
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub fn foo() {}
            pub fn bar() {}
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::{foo, bar};
            pub fn cmd_orchestrate() { foo(); bar(); }
            "#,
        ),
    ]);
    let cp = make_config(SingleTouchpointMode::Off);
    let findings = run_check_c(&ws, &three_layer(), &cp, &empty_cfg_test());
    assert!(
        extract_c(&findings).is_empty(),
        "Off mode should skip the check entirely, got {findings:?}"
    );
}

// ── Severity: error mode produces findings (severity tested via projection) ──

#[test]
fn check_c_severity_error_emits_finding() {
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub fn foo() {}
            pub fn bar() {}
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::{foo, bar};
            pub fn cmd_orchestrate() { foo(); bar(); }
            "#,
        ),
    ]);
    let cp = make_config(SingleTouchpointMode::Error);
    let findings = run_check_c(&ws, &three_layer(), &cp, &empty_cfg_test());
    assert_eq!(extract_c(&findings).len(), 1, "got {findings:?}");
}

// ── Loop calling the same target multiple times → silent ──────

#[test]
fn check_c_loop_single_target_silent() {
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn process(_x: u32) {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::process;
            pub fn cmd_batch() {
                for x in 0..10 { process(x); }
            }
            "#,
        ),
    ]);
    let cp = make_config(SingleTouchpointMode::Warn);
    let findings = run_check_c(&ws, &three_layer(), &cp, &empty_cfg_test());
    assert!(
        extract_c(&findings).is_empty(),
        "single target hit by multiple call sites is one touchpoint, got {findings:?}"
    );
}

// ── Finding lists every touchpoint ───────────────────────────

#[test]
fn check_c_finding_lists_touchpoints() {
    let ws = build_workspace(&[
        (
            "src/application/session.rs",
            r#"
            pub fn alpha() {}
            pub fn beta() {}
            pub fn gamma() {}
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::{alpha, beta, gamma};
            pub fn cmd_three() { alpha(); beta(); gamma(); }
            "#,
        ),
    ]);
    let cp = make_config(SingleTouchpointMode::Warn);
    let findings = run_check_c(&ws, &three_layer(), &cp, &empty_cfg_test());
    let pairs = extract_c(&findings);
    assert_eq!(pairs.len(), 1, "got {findings:?}");
    assert_eq!(pairs[0].1.len(), 3);
}
