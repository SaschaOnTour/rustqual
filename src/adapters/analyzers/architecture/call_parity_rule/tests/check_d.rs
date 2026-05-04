//! Tests for Check D — multiplicity mismatch.
//!
//! Check D fires when a target pub-fn IS in every adapter's coverage
//! (so Check B is silent) but the per-adapter handler counts diverge
//! — typical case: cli has two handlers (`cmd_search`, `cmd_grep`)
//! both reaching `session.search` while mcp has only `handle_search`.

use super::support::{
    build_workspace, empty_cfg_test, four_layer, globset, ports_app_cli_mcp, run_check_d,
};
use crate::adapters::analyzers::architecture::compiled::CompiledCallParity;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use std::collections::HashSet;

fn make_config(adapters: &[&str]) -> CompiledCallParity {
    CompiledCallParity {
        adapters: adapters.iter().map(|s| s.to_string()).collect(),
        target: "application".to_string(),
        call_depth: 3,
        exclude_targets: globset(&[]),
        transparent_wrappers: HashSet::new(),
        transparent_macros: HashSet::new(),
        single_touchpoint: crate::config::architecture::SingleTouchpointMode::default(),
    }
}

fn extract_d(findings: &[MatchLocation]) -> Vec<(String, Vec<(String, usize)>)> {
    findings
        .iter()
        .filter_map(|f| match &f.kind {
            ViolationKind::CallParityMultiplicityMismatch {
                target_fn,
                counts_per_adapter,
                ..
            } => Some((target_fn.clone(), counts_per_adapter.clone())),
            _ => None,
        })
        .collect()
}

// ── Two adapters, asymmetric multiplicity ────────────────────────

#[test]
fn check_d_alias_in_one_adapter() {
    // cli has cmd_search and cmd_grep both → session.search.
    // mcp has handle_search only → session.search.
    // counts: cli=2, mcp=1 → finding for session.search.
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            pub fn cmd_grep() { search(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn handle_search() { search(); }
            "#,
        ),
    ]);
    let cp = make_config(&["cli", "mcp"]);
    let findings = run_check_d(&ws, &four_layer(), &cp, &empty_cfg_test());
    let pairs = extract_d(&findings);
    assert_eq!(pairs.len(), 1, "got {findings:?}");
    let (target, counts) = &pairs[0];
    assert!(target.ends_with("session::search"));
    let cli_count = counts.iter().find(|(a, _)| a == "cli").map(|(_, c)| *c);
    let mcp_count = counts.iter().find(|(a, _)| a == "mcp").map(|(_, c)| *c);
    assert_eq!(cli_count, Some(2));
    assert_eq!(mcp_count, Some(1));
}

// ── Balanced multiplicity → silent ───────────────────────────────

#[test]
fn check_d_balanced_fan_in_no_finding() {
    // cli: cmd_search + cmd_grep → search; mcp: handle_search +
    // handle_grep → search. counts match (2,2) → no Check D finding.
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            pub fn cmd_grep() { search(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn handle_search() { search(); }
            pub fn handle_grep() { search(); }
            "#,
        ),
    ]);
    let cp = make_config(&["cli", "mcp"]);
    let findings = run_check_d(&ws, &four_layer(), &cp, &empty_cfg_test());
    assert!(
        extract_d(&findings).is_empty(),
        "balanced fan-in should be silent, got {findings:?}"
    );
}

// ── Three adapters, one diverges ─────────────────────────────────

#[test]
fn check_d_three_adapters_one_diverges() {
    // cli=2, rest=2, mcp=1 → finding citing the divergence.
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            pub fn cmd_grep() { search(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn handle_search() { search(); }
            "#,
        ),
        (
            "src/rest/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn post_search() { search(); }
            pub fn post_grep() { search(); }
            "#,
        ),
    ]);
    let cp = make_config(&["cli", "mcp", "rest"]);
    let findings = run_check_d(&ws, &four_layer(), &cp, &empty_cfg_test());
    let pairs = extract_d(&findings);
    assert_eq!(pairs.len(), 1, "got {findings:?}");
    let counts = &pairs[0].1;
    let cli_count = counts.iter().find(|(a, _)| a == "cli").map(|(_, c)| *c);
    let mcp_count = counts.iter().find(|(a, _)| a == "mcp").map(|(_, c)| *c);
    let rest_count = counts.iter().find(|(a, _)| a == "rest").map(|(_, c)| *c);
    assert_eq!(cli_count, Some(2));
    assert_eq!(mcp_count, Some(1));
    assert_eq!(rest_count, Some(2));
}

// ── Deprecated alias not counted (v1.2.1) ────────────────────────

#[test]
fn check_d_skips_deprecated_alias() {
    // cli has cmd_search (live) + cmd_grep (deprecated alias) → both
    // would touch session.search. With deprecation-exclusion, cmd_grep
    // is filtered: cli effective count = 1, mcp = 1 → no D finding.
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            #[deprecated]
            pub fn cmd_grep() { search(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn handle_search() { search(); }
            "#,
        ),
    ]);
    let cp = make_config(&["cli", "mcp"]);
    let findings = run_check_d(&ws, &four_layer(), &cp, &empty_cfg_test());
    assert!(
        extract_d(&findings).is_empty(),
        "deprecated alias should be excluded from D's count, got {findings:?}"
    );
}

// ── Distinct from B: capability missing entirely emits B not D ───

#[test]
fn check_d_distinct_from_b() {
    // cli reaches session.search; mcp doesn't reach it at all.
    // → Check B finding (capability missing). Check D MUST be silent
    // because D only fires when target is in every adapter's coverage
    // but counts differ.
    let ws = build_workspace(&[
        ("src/application/session.rs", "pub fn search() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::session::search;
            pub fn cmd_search() { search(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            // mcp doesn't touch search at all
            pub fn handle_other() {}
            "#,
        ),
    ]);
    let cp = make_config(&["cli", "mcp"]);
    let findings = run_check_d(&ws, &four_layer(), &cp, &empty_cfg_test());
    assert!(
        extract_d(&findings).is_empty(),
        "Check D should not fire when target missing entirely from an adapter (that's Check B's job), got {findings:?}"
    );
}

#[test]
fn check_d_uses_anchor_as_capability_for_multiplicity() {
    // cli has TWO handlers dispatching `dyn Handler.handle()`.
    // mcp has ONE handler doing the same. Both adapters reach the
    // anchor (Check B silent), but multiplicity diverges: cli=2,
    // mcp=1. Check D must fire on the trait-method anchor as the
    // capability — not on the concrete impl, which the dispatch
    // never directly calls. Without anchor iteration this drift
    // would be silent.
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn cmd_dispatch_a(h: &dyn Handler) { h.handle(); }
            pub fn cmd_dispatch_b(h: &dyn Handler) { h.handle(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn mcp_dispatch(h: &dyn Handler) { h.handle(); }
            "#,
        ),
    ]);
    let cp = make_config(&["cli", "mcp"]);
    let findings = run_check_d(&ws, &ports_app_cli_mcp(), &cp, &empty_cfg_test());
    let entries = extract_d(&findings);
    let anchor = "crate::ports::handler::Handler::handle";
    let anchor_entry = entries
        .iter()
        .find(|(target, _)| target == anchor)
        .unwrap_or_else(|| panic!("Check D missed anchor multiplicity drift, got {entries:?}"));
    let counts: std::collections::HashMap<&str, usize> = anchor_entry
        .1
        .iter()
        .map(|(a, c)| (a.as_str(), *c))
        .collect();
    assert_eq!(counts.get("cli"), Some(&2));
    assert_eq!(counts.get("mcp"), Some(&1));
}

#[test]
fn check_d_surfaces_concrete_multiplicity_when_all_adapters_call_direct() {
    // Sister-fix of check_b's mixed-form scenario, scoped to Check D's
    // semantic ("all adapters reach the target, counts differ"). Both
    // cli and mcp call the concrete `LoggingHandler::handle()` directly
    // via UFCS — no `dyn Trait` dispatch anywhere. cli has TWO
    // handlers, mcp has ONE → multiplicity drift on the concrete
    // canonical.
    //
    // Without the conditional skip, `is_anchor_backed_concrete`
    // unconditionally drops the concrete from Check D's iteration
    // (the trait Handler has an overriding impl in target → the skip
    // fires). With both adapters reaching only via concrete, the
    // anchor pass produces no counts either, so Check D goes silent
    // — masking a clear-cut multiplicity drift.
    //
    // Fix mirrors check_b: skip only when no adapter has the concrete
    // in coverage. When adapters reach via direct concrete, the
    // concrete pass runs and the counts diverge (cli=2, mcp=1).
    let ws = build_workspace(&[
        (
            "src/ports/handler.rs",
            "pub trait Handler { fn handle(&self); }",
        ),
        (
            "src/application/logging.rs",
            r#"
            use crate::ports::handler::Handler;
            pub struct LoggingHandler;
            impl Handler for LoggingHandler { fn handle(&self) {} }
            "#,
        ),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::logging::LoggingHandler;
            pub fn cmd_log_a() { LoggingHandler::handle(&LoggingHandler); }
            pub fn cmd_log_b() { LoggingHandler::handle(&LoggingHandler); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::logging::LoggingHandler;
            pub fn mcp_log() { LoggingHandler::handle(&LoggingHandler); }
            "#,
        ),
    ]);
    let cp = make_config(&["cli", "mcp"]);
    let findings = run_check_d(&ws, &ports_app_cli_mcp(), &cp, &empty_cfg_test());
    let entries = extract_d(&findings);
    let concrete = "crate::application::logging::LoggingHandler::handle";
    let concrete_entry = entries
        .iter()
        .find(|(target, _)| target == concrete)
        .unwrap_or_else(|| {
            panic!(
                "Check D must surface concrete multiplicity drift when all adapters reach via direct concrete; got {entries:?}"
            )
        });
    let counts: std::collections::HashMap<&str, usize> = concrete_entry
        .1
        .iter()
        .map(|(a, c)| (a.as_str(), *c))
        .collect();
    assert_eq!(
        counts.get("cli"),
        Some(&2),
        "concrete count for cli (two direct UFCS callers); got {counts:?}"
    );
    assert_eq!(
        counts.get("mcp"),
        Some(&1),
        "concrete count for mcp (one direct UFCS caller); got {counts:?}"
    );
}
