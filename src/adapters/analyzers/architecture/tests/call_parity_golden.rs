//! End-to-end snapshot test for the `[architecture.call_parity]`
//! golden example at `examples/architecture/call_parity/`.
//!
//! The fixture is designed to produce exactly two architecture findings:
//! - `no_delegation` on `post_list` (REST inlines the list operation).
//! - `missing_adapter` on `application::list::list_items` (REST never
//!   calls it, so the coverage set is incomplete).
//!
//! Additionally, `cmd_debug` in the CLI adapter carries
//! `// qual:allow(architecture)` so it emits a raw finding that must
//! then be suppressed by the architecture-dimension suppression
//! pipeline. The end-to-end `cargo run -- examples/...` path exercises
//! that filter; this unit test exercises the raw-finding layer so a
//! regression in either step is localised.

use crate::adapters::analyzers::architecture::call_parity_rule;
use crate::adapters::analyzers::architecture::compiled::compile_architecture;
use crate::config::Config;
use crate::ports::{AnalysisContext, ParsedFile};
use std::fs;
use std::path::{Path, PathBuf};

fn example_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("architecture")
        .join("call_parity")
}

fn load_parsed_file(root: &Path, rel: &str) -> ParsedFile {
    let abs = root.join(rel);
    let content = fs::read_to_string(&abs)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", abs.display()));
    let ast: syn::File = syn::parse_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", abs.display()));
    ParsedFile {
        path: rel.to_string(),
        content,
        ast,
    }
}

fn load_golden_config(root: &Path) -> Config {
    let toml_str = fs::read_to_string(root.join("rustqual.toml"))
        .expect("failed to read example rustqual.toml");
    let mut config: Config = toml::from_str(&toml_str).expect("failed to parse example config");
    config.compile();
    config
}

fn parsed_files(root: &Path) -> Vec<ParsedFile> {
    [
        "src/application/stats.rs",
        "src/application/list.rs",
        "src/application/mod.rs",
        "src/cli/handlers.rs",
        "src/cli/mod.rs",
        "src/mcp/handlers.rs",
        "src/mcp/mod.rs",
        "src/rest/handlers.rs",
        "src/rest/mod.rs",
    ]
    .iter()
    .map(|rel| load_parsed_file(root, rel))
    .collect()
}

#[test]
fn call_parity_golden_example_produces_expected_findings() {
    let root = example_root();
    let files = parsed_files(&root);
    let config = load_golden_config(&root);
    let ctx = AnalysisContext {
        files: &files,
        config: &config,
    };
    let compiled = compile_architecture(&config.architecture).expect("compile architecture config");
    let findings = call_parity_rule::collect_findings(&ctx, &compiled);

    // Expected set: one no_delegation on post_list + one missing_adapter
    // on application::list::list_items. cmd_debug also produces a raw
    // no_delegation finding — suppressed by mark_architecture_suppressions
    // in the full pipeline, visible here as an extra raw finding.
    let no_delegation: Vec<_> = findings
        .iter()
        .filter(|f| f.rule_id == "architecture/call_parity/no_delegation")
        .collect();
    let missing_adapter: Vec<_> = findings
        .iter()
        .filter(|f| f.rule_id == "architecture/call_parity/missing_adapter")
        .collect();

    let no_delegation_fns: std::collections::HashSet<String> =
        no_delegation.iter().map(|f| f.message.clone()).collect();
    assert!(
        no_delegation_fns.iter().any(|m| m.contains("post_list")),
        "expected no_delegation for post_list, got {:?}",
        no_delegation
    );
    assert!(
        no_delegation_fns.iter().any(|m| m.contains("cmd_debug")),
        "expected raw no_delegation for cmd_debug (suppressed by \
         architecture suppression pipeline later), got {:?}",
        no_delegation
    );

    assert_eq!(
        missing_adapter.len(),
        1,
        "expected exactly one missing_adapter finding, got {:?}",
        missing_adapter
    );
    let ma = missing_adapter[0];
    assert!(
        ma.message.contains("list_items"),
        "missing_adapter must target list_items, got message = {}",
        ma.message
    );
    assert!(
        ma.message.contains("rest"),
        "missing_adapter must mention rest as missing, got message = {}",
        ma.message
    );
}
