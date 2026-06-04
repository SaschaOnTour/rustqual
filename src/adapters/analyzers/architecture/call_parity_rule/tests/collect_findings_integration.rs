//! Integration test for the `collect_findings` orchestrator — the only
//! place that gates hint enrichment behind `!check_b_hits.is_empty()`.
//! (The unit-level `tests/check_b/hints.rs` exercises enrichment through
//! the manual `run_check_b` path, which has no such guard.)

use super::support::{cli_mcp_config, ports_app_cli_mcp};
use crate::adapters::analyzers::architecture::call_parity_rule::collect_findings;
use crate::adapters::analyzers::architecture::compiled::CompiledArchitecture;
use crate::adapters::analyzers::architecture::layer_rule::UnmatchedBehavior;
use crate::config::Config;
use crate::ports::{AnalysisContext, ParsedFile};
use globset::GlobSet;
use std::collections::HashMap;

fn parsed(path: &str, src: &str) -> ParsedFile {
    ParsedFile {
        path: path.to_string(),
        content: src.to_string(),
        ast: syn::parse_file(src).expect("parse file"),
    }
}

fn compiled_with_call_parity() -> CompiledArchitecture {
    CompiledArchitecture {
        layers: ports_app_cli_mcp(),
        reexport_points: GlobSet::empty(),
        unmatched_behavior: UnmatchedBehavior::CompositionRoot,
        external_exact: HashMap::new(),
        external_glob: Vec::new(),
        forbidden: Vec::new(),
        trait_contracts: Vec::new(),
        call_parity: Some(cli_mcp_config(2)),
    }
}

#[test]
fn collect_findings_enriches_missing_adapter_findings_with_hints() {
    // cli reaches `Session::open` directly; mcp reaches it only through a
    // PRIVATE `#[tool]` fn → a missing-adapter finding for mcp plus a hint
    // naming that private fn. `collect_findings` only enriches when
    // `!check_b_hits.is_empty()` — deleting the `!` skips enrichment for
    // real findings, so the hint text disappears from the message.
    let files = vec![
        parsed(
            "src/application/session.rs",
            "pub struct Session;\npub struct MyErr;\nimpl Session { pub fn open(_p: &str) -> Result<Self, MyErr> { todo!() } }",
        ),
        parsed(
            "src/cli/handlers.rs",
            "use crate::application::session::Session;\npub fn cmd_search() { let _ = Session::open(\"/p\"); }",
        ),
        parsed(
            "src/mcp/server.rs",
            "use crate::application::session::{Session, MyErr};\npub struct Server;\nimpl Server { #[tool(description = \"search\")] async fn search(&self) -> Result<(), MyErr> { let _ = Session::open(\"/p\"); Ok(()) } }",
        ),
    ];
    let config = Config::default();
    let ctx = AnalysisContext {
        files: &files,
        config: &config,
    };
    let compiled = compiled_with_call_parity();
    let findings = collect_findings(&ctx, &compiled);
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("transitively reach")),
        "missing-adapter finding carries the private-fn hint: {:?}",
        findings.iter().map(|f| &f.message).collect::<Vec<_>>()
    );
}
