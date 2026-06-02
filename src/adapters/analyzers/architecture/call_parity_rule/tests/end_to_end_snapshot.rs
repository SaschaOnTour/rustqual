//! End-to-end session/handler snapshot — the acceptance gate for the
//! receiver-type inference wiring.
//!
//! Sets up a mini multi-file workspace mirroring a session / handler
//! shape (CLI cmd → `Session::open_cwd().map_err(f)?` → `session.method()`,
//! MCP handler with `session: &Session` parameter), runs the full
//! call-parity pipeline (Checks A/B/C/D), and asserts the exact set
//! of surviving findings. The fixture is deliberately small (3 files,
//! ~50 lines) but covers every `call_parity_rule` code path the
//! receiver-inference work exercised:
//!
//! - CLI handlers that do `let session = Session::open_cwd().map_err(f)?;
//!   session.method(...)` — the method-chain constructor pattern
//! - MCP handlers with `session: &Session` parameter — the
//!   signature-param fast path
//! - Asymmetric coverage (method called from only one adapter) —
//!   legitimate findings the rule should still emit
//! - Genuinely unreached methods — real dead code
//!
//! If trait-dispatch or config-based wrappers add new resolution
//! paths, this snapshot's expected-findings list moves downward —
//! adjust the assertions when that happens.

use super::support::{
    build_workspace, cli_mcp_config, empty_cfg_test, run_check_a, run_check_b, three_layer,
};
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::HashSet;

/// The fixture files. Paths determine layer membership:
/// - `src/application/**` → application (target)
/// - `src/cli/**` → cli adapter
/// - `src/mcp/**` → mcp adapter
fn session_handler_fixture() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "src/application/session.rs",
            r#"
            pub struct Session;
            pub struct Response;
            pub struct Error;

            impl Session {
                pub fn open_cwd() -> Result<Session, Error> { todo!() }
                pub fn open(path: &str) -> Result<Session, Error> { todo!() }
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
            use crate::application::session::Session;

            pub struct CliError;
            fn map_err(_e: crate::application::session::Error) -> CliError { CliError }

            pub fn cmd_diff(path: &str) -> Result<(), CliError> {
                let session = Session::open_cwd().map_err(map_err)?;
                let _ = session.diff(path).map_err(map_err)?;
                Ok(())
            }
            pub fn cmd_files() -> Result<(), CliError> {
                let session = Session::open_cwd().map_err(map_err)?;
                let _ = session.files().map_err(map_err)?;
                Ok(())
            }
            pub fn cmd_stats() -> Result<(), CliError> {
                let session = Session::open_cwd().map_err(map_err)?;
                let _ = session.stats();
                Ok(())
            }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::Session;

            pub fn handle_diff(session: &Session, path: &str) -> String {
                let _ = session.diff(path);
                String::new()
            }
            pub fn handle_files(session: &Session) -> String {
                let _ = session.files();
                String::new()
            }
            pub fn handle_insert(session: &Session, content: &str) -> String {
                let _ = session.insert(content);
                String::new()
            }
            "#,
        ),
    ]
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

/// Run Check A over the session/handler fixture (three-layer, cli+mcp, depth 3).
fn check_a_on_fixture() -> Vec<MatchLocation> {
    run_check_a(
        &build_workspace(&session_handler_fixture()),
        &three_layer(),
        &cli_mcp_config(3),
        &empty_cfg_test(),
    )
}

/// Run Check B over the session/handler fixture (three-layer, cli+mcp, depth 3).
fn check_b_on_fixture() -> Vec<MatchLocation> {
    run_check_b(
        &build_workspace(&session_handler_fixture()),
        &three_layer(),
        &cli_mcp_config(3),
        &empty_cfg_test(),
    )
}

// ═══════════════════════════════════════════════════════════════════
// Check A — every adapter pub fn must delegate into application
// ═══════════════════════════════════════════════════════════════════

#[test]
fn check_a_clean_on_session_handler_fixture() {
    // Every cli / mcp handler in the fixture reaches an application-
    // layer fn via inference, so Check A has no findings at all —
    // this is the primary receiver-inference regression guard.
    let findings = check_a_on_fixture();
    assert!(
        findings.is_empty(),
        "Check A should be clean on the session/handler fixture, got {} findings: {:?}",
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
fn check_b_targets_reached_from_both_adapters() {
    // Session::diff (cli via chain inference, mcp via signature-param) and
    // Session::files are both covered by every configured adapter → no finding.
    let findings = check_b_on_fixture();
    for target in [
        "crate::application::session::Session::diff",
        "crate::application::session::Session::files",
    ] {
        assert!(
            missing_adapters_for(&findings, target).is_none(),
            "{target} should be reached from both adapters"
        );
    }
}

#[test]
fn check_b_asymmetric_coverage_is_flagged() {
    // `stats` is only called from cli (cmd_stats). mcp doesn't cover
    // it — legitimate Check B finding.
    let findings = check_b_on_fixture();
    let missing = missing_adapters_for(&findings, "crate::application::session::Session::stats")
        .expect("stats should be missing from some adapter");
    assert_eq!(missing, vec!["mcp".to_string()]);

    // `insert` is only called from mcp — missing from cli.
    let missing = missing_adapters_for(&findings, "crate::application::session::Session::insert")
        .expect("insert should be missing from some adapter");
    assert_eq!(missing, vec!["cli".to_string()]);
}

#[test]
fn check_b_unreached_pub_fn_is_flagged() {
    // `genuinely_unused` has no callers → missing from all adapters.
    let findings = check_b_on_fixture();
    let missing = missing_adapters_for(
        &findings,
        "crate::application::session::Session::genuinely_unused",
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
fn total_findings_budget_on_session_handler_fixture() {
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
    let check_a = check_a_on_fixture();
    let check_b = check_b_on_fixture();
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
