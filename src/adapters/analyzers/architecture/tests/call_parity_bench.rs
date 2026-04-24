//! Benchmark regression guard for `[architecture.call_parity]`.
//!
//! Runs the full call-parity pipeline (call-graph build + Check A +
//! Check B + receiver-type tracking) on rustqual's own source tree and
//! asserts the pass stays below a wall-time ceiling. Sized to catch
//! O(n²) regressions in the BFS / reverse-graph code before they hit
//! users on ~50k-LOC external projects like rlm.
//!
//! Marked `#[ignore]` so it runs only under `cargo test -- --ignored`
//! (or `cargo nextest run --run-ignored=only`). Keeps CI wall-time
//! stable while still giving us a one-command regression check before
//! releases.
//!
//! The fixture config doesn't reflect rustqual's actual architecture —
//! we just need *some* adapter layers + a target so the check exercises
//! real graph construction. The timing is what we assert, not the
//! finding set.

#![cfg(test)]

use crate::adapters::analyzers::architecture::call_parity_rule;
use crate::adapters::analyzers::architecture::compiled::compile_architecture;
use crate::config::Config;
use crate::ports::{AnalysisContext, ParsedFile};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const BENCH_SOFT_LIMIT: std::time::Duration = std::time::Duration::from_secs(3);

fn rustqual_src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn load_rust_files(root: &Path) -> Vec<ParsedFile> {
    let mut out = Vec::new();
    collect_rs_paths(root, &mut out);
    out
}

/// Walk the directory tree rooted at `current` recursively, parsing each
/// `.rs` file into a `ParsedFile` with the project-root-relative path.
// qual:recursive
fn collect_rs_paths(current: &Path, out: &mut Vec<ParsedFile>) {
    let dir = fs::read_dir(current)
        .unwrap_or_else(|e| panic!("failed to read dir {}: {e}", current.display()));
    for entry in dir {
        let entry =
            entry.unwrap_or_else(|e| panic!("failed to read entry in {}: {e}", current.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_rs_paths(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(parse_one(&path));
        }
    }
}

/// Read + parse a single `.rs` file into a `ParsedFile`. The benchmark
/// is meant as a regression guard over rustqual's own source — silent
/// skips would under-measure the real call_parity cost, so any I/O or
/// parse failure panics with the offending path rather than hiding.
fn parse_one(abs: &Path) -> ParsedFile {
    let content =
        fs::read_to_string(abs).unwrap_or_else(|e| panic!("failed to read {}: {e}", abs.display()));
    let ast: syn::File = syn::parse_str(&content)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", abs.display()));
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rel = abs
        .strip_prefix(manifest)
        .unwrap_or_else(|e| panic!("{} is not under {}: {e}", abs.display(), manifest.display()))
        .to_string_lossy()
        .replace('\\', "/");
    ParsedFile {
        path: rel,
        content,
        ast,
    }
}

fn bench_config() -> Config {
    // A minimal call_parity setup: rustqual's own layers + a synthetic
    // adapter/target split. Not meaningful as a drift check, but it
    // forces the full pipeline to run so timing reflects reality.
    let toml_str = r#"
        [architecture]
        enabled = true

        [architecture.layers]
        order = ["domain", "ports", "adapters", "app", "cli"]

        [architecture.layers.domain]
        paths = ["src/domain/**"]

        [architecture.layers.ports]
        paths = ["src/ports/**"]

        [architecture.layers.adapters]
        paths = ["src/adapters/**"]

        [architecture.layers.app]
        paths = ["src/app/**"]

        [architecture.layers.cli]
        paths = ["src/cli/**"]

        [architecture.call_parity]
        adapters = ["cli", "adapters"]
        target = "app"
        call_depth = 3
    "#;
    let mut config: Config = toml::from_str(toml_str).expect("parse bench config");
    config.compile();
    config
}

#[test]
#[ignore = "performance regression guard — run via `cargo test -- --ignored` before release"]
fn benchmark_call_parity_on_self_analysis() {
    let files = load_rust_files(&rustqual_src_root());
    assert!(
        files.len() > 50,
        "self-analysis fixture looks empty — found {} files",
        files.len()
    );
    let config = bench_config();
    let ctx = AnalysisContext {
        files: &files,
        config: &config,
    };
    let compiled = compile_architecture(&config.architecture).expect("compile");
    let start = Instant::now();
    let _findings = call_parity_rule::collect_findings(&ctx, &compiled);
    let elapsed = start.elapsed();
    assert!(
        elapsed < BENCH_SOFT_LIMIT,
        "call_parity pass regressed: {elapsed:?} > {BENCH_SOFT_LIMIT:?} — \
         likely an O(n²) schleicher in the BFS / reverse-graph code"
    );
    eprintln!(
        "call_parity self-analysis: {} files, {elapsed:?}",
        files.len()
    );
}
