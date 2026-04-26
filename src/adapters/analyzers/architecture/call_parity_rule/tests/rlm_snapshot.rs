//! End-to-end rlm-shape snapshot — the external acceptance gate for the
//! Task 1.6 inference wiring.
//!
//! Sets up a mini multi-file workspace that mirrors rlm's session /
//! handler pattern (the one the original bug report called out), runs
//! the full Check A + Check B pipeline, and asserts the exact set of
//! surviving findings. The fixture is deliberately small (3 files,
//! ~50 lines of rlm-shaped source) but covers every `call_parity_rule`
//! code path the rlm bug exercised:
//!
//! - CLI handlers that do `let session = RlmSession::open_cwd().map_err(f)?;
//!   session.method(...)` — the method-chain constructor pattern
//! - MCP handlers with `session: &RlmSession` parameter — the
//!   signature-param fast path
//! - Asymmetric coverage (method called from only one adapter) —
//!   legitimate findings the rule should still emit
//! - Genuinely unreached methods — real dead code
//!
//! If Stage 2 trait-dispatch or Stage 3 config-based wrappers add new
//! resolution paths, this snapshot's expected-findings list moves
//! downward — adjust the assertions when that happens.

use super::support::{build_workspace, empty_cfg_test, globset, run_check_a, run_check_b};
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::layer_rule::LayerDefinitions;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::HashSet;

/// The fixture files. Paths determine layer membership:
/// - `src/application/**` → application (target)
/// - `src/cli/**` → cli adapter
/// - `src/mcp/**` → mcp adapter
fn rlm_fixture() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "src/application/session.rs",
            r#"
            pub struct RlmSession;
            pub struct Response;
            pub struct Error;

            impl RlmSession {
                pub fn open_cwd() -> Result<RlmSession, Error> { todo!() }
                pub fn open(path: &str) -> Result<RlmSession, Error> { todo!() }
                pub fn diff(&self, path: &str) -> Result<Response, Error> { todo!() }
                pub fn files(&self) -> Result<Response, Error> { todo!() }
                pub fn insert(&self, content: &str) -> Result<Response, Error> { todo!() }
                pub fn stats(&self) -> Response { todo!() }
                pub fn genuinely_unused(&self) {}
            }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::RlmSession;

            pub struct CliError;
            fn map_err(_e: crate::application::session::Error) -> CliError { CliError }

            pub fn cmd_diff(path: &str) -> Result<(), CliError> {
                let session = RlmSession::open_cwd().map_err(map_err)?;
                let _ = session.diff(path).map_err(map_err)?;
                Ok(())
            }
            pub fn cmd_files() -> Result<(), CliError> {
                let session = RlmSession::open_cwd().map_err(map_err)?;
                let _ = session.files().map_err(map_err)?;
                Ok(())
            }
            pub fn cmd_stats() -> Result<(), CliError> {
                let session = RlmSession::open_cwd().map_err(map_err)?;
                let _ = session.stats();
                Ok(())
            }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::RlmSession;

            pub fn handle_diff(session: &RlmSession, path: &str) -> String {
                let _ = session.diff(path);
                String::new()
            }
            pub fn handle_files(session: &RlmSession) -> String {
                let _ = session.files();
                String::new()
            }
            pub fn handle_insert(session: &RlmSession, content: &str) -> String {
                let _ = session.insert(content);
                String::new()
            }
            "#,
        ),
    ]
}

fn rlm_layers() -> LayerDefinitions {
    LayerDefinitions::new(
        vec![
            "application".to_string(),
            "cli".to_string(),
            "mcp".to_string(),
        ],
        vec![
            ("application".to_string(), globset(&["src/application/**"])),
            ("cli".to_string(), globset(&["src/cli/**"])),
            ("mcp".to_string(), globset(&["src/mcp/**"])),
        ],
    )
}

fn rlm_config() -> CompiledCallParity {
    CompiledCallParity {
        adapters: vec!["cli".to_string(), "mcp".to_string()],
        target: "application".to_string(),
        call_depth: 3,
        exclude_targets: globset(&[]),
        transparent_wrappers: HashSet::new(),
        transparent_macros: HashSet::new(),
    }
}

fn missing_adapters_for(findings: &[MatchLocation], target_fn: &str) -> Option<Vec<String>> {
    findings.iter().find_map(|f| match &f.kind {
        ViolationKind::CallParityMissingAdapter {
            target_fn: tf,
            missing_adapters,
            ..
        } if tf == target_fn => Some(missing_adapters.clone()),
        _ => None,
    })
}

// ═══════════════════════════════════════════════════════════════════
// Check A — every adapter pub fn must delegate into application
// ═══════════════════════════════════════════════════════════════════

#[test]
fn rlm_snapshot_check_a_no_spurious_findings() {
    // After Task 1.6, every cli / mcp handler in the fixture reaches an
    // application-layer fn via inference. So Check A has no findings at
    // all — this is the primary rlm-bug regression guard.
    let ws = build_workspace(&rlm_fixture());
    let findings = run_check_a(&ws, &rlm_layers(), &rlm_config(), &empty_cfg_test());
    assert!(
        findings.is_empty(),
        "Check A should be clean on rlm-shape fixture, got {} findings: {:?}",
        findings.len(),
        findings
            .iter()
            .map(|f| format!("{}:{}", f.file, f.line))
            .collect::<Vec<_>>()
    );
}

// ═══════════════════════════════════════════════════════════════════
// Check B — target pub fns must be reached from every configured adapter
// ═══════════════════════════════════════════════════════════════════

#[test]
fn rlm_snapshot_check_b_diff_reached_from_both_adapters() {
    // The hero case: Session::diff is called from both cli (via chain
    // inference) and mcp (via signature-param). Both adapters cover it
    // → no finding.
    let ws = build_workspace(&rlm_fixture());
    let findings = run_check_b(&ws, &rlm_layers(), &rlm_config(), &empty_cfg_test());
    let missing = missing_adapters_for(&findings, "crate::application::session::RlmSession::diff");
    assert!(
        missing.is_none(),
        "RlmSession::diff should be reached from both adapters, got missing={:?}",
        missing
    );
}

#[test]
fn rlm_snapshot_check_b_files_reached_from_both_adapters() {
    let ws = build_workspace(&rlm_fixture());
    let findings = run_check_b(&ws, &rlm_layers(), &rlm_config(), &empty_cfg_test());
    let missing = missing_adapters_for(&findings, "crate::application::session::RlmSession::files");
    assert!(missing.is_none(), "RlmSession::files should be reached");
}

#[test]
fn rlm_snapshot_check_b_asymmetric_coverage_flagged() {
    // `stats` is only called from cli (cmd_stats). mcp doesn't cover
    // it — legitimate Check B finding.
    let ws = build_workspace(&rlm_fixture());
    let findings = run_check_b(&ws, &rlm_layers(), &rlm_config(), &empty_cfg_test());
    let missing = missing_adapters_for(&findings, "crate::application::session::RlmSession::stats")
        .expect("stats should be missing from some adapter");
    assert_eq!(missing, vec!["mcp".to_string()]);

    // `insert` is only called from mcp — missing from cli.
    let missing =
        missing_adapters_for(&findings, "crate::application::session::RlmSession::insert")
            .expect("insert should be missing from some adapter");
    assert_eq!(missing, vec!["cli".to_string()]);
}

#[test]
fn rlm_snapshot_check_b_unreached_pub_fn_is_flagged() {
    // `genuinely_unused` has no callers → missing from all adapters.
    let ws = build_workspace(&rlm_fixture());
    let findings = run_check_b(&ws, &rlm_layers(), &rlm_config(), &empty_cfg_test());
    let missing = missing_adapters_for(
        &findings,
        "crate::application::session::RlmSession::genuinely_unused",
    )
    .expect("genuinely_unused must be flagged");
    let set: HashSet<String> = missing.into_iter().collect();
    assert!(set.contains("cli"));
    assert!(set.contains("mcp"));
}

// ═══════════════════════════════════════════════════════════════════
// Budget assertion — total finding count on the fixture
// ═══════════════════════════════════════════════════════════════════

#[test]
fn rlm_snapshot_total_findings_budget() {
    // The fixture has 7 application pub fns. Under the configured
    // `application` layer:
    //   - open:             reached from nobody       → missing [cli, mcp]
    //   - open_cwd:         reached only from cli      → missing [mcp]
    //   - diff:             reached from both          → clean
    //   - files:            reached from both          → clean
    //   - insert:           reached only from mcp      → missing [cli]
    //   - stats:            reached only from cli      → missing [mcp]
    //   - genuinely_unused: reached from nobody       → missing [cli, mcp]
    // → 5 Check B findings, 0 Check A findings.
    //
    // If this budget ticks upward, inspect the new findings before
    // adjusting the number — the Stage 1 implementation should not
    // regress this count.
    let ws = build_workspace(&rlm_fixture());
    let layers = rlm_layers();
    let cp = rlm_config();
    let check_a = run_check_a(&ws, &layers, &cp, &empty_cfg_test());
    let check_b = run_check_b(&ws, &layers, &cp, &empty_cfg_test());
    assert_eq!(check_a.len(), 0, "Check A: {:?}", check_a);
    assert_eq!(
        check_b.len(),
        5,
        "Check B count drifted: {:?}",
        check_b
            .iter()
            .filter_map(|f| match &f.kind {
                ViolationKind::CallParityMissingAdapter { target_fn, .. } =>
                    Some(target_fn.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
    );
}
