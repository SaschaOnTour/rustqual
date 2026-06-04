//! Tests for the call-parity hint subsystem (`hint/`): the
//! `CandidateCollector` visitor that records promotable private fns and
//! the `format_hint` renderer. Each test isolates one visitor predicate
//! (visibility chain, test-attr skip, impl/mod nesting) or the
//! singular/plural rendering branch so the corresponding mutant dies.

use super::support::{borrowed_files, build_workspace, three_layer};
use crate::adapters::analyzers::architecture::call_parity_rule::hint::{
    collect_private_candidates, format_hint, PrivateCandidate,
};
use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::{
    collect_workspace_module_paths, WorkspaceLookup,
};
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::collect_crate_root_modules;
use std::collections::HashSet;

/// Collect promotable private candidates from a single in-memory file
/// under the `three_layer` layout (application/cli/mcp), no cfg-test
/// files and no transparent wrappers. Integration: builds the
/// `WorkspaceLookup` the collector needs, then delegates.
fn candidates_of(path: &str, src: &str) -> Vec<PrivateCandidate> {
    let ws = build_workspace(&[(path, src)]);
    let borrowed = borrowed_files(&ws);
    let crate_root_modules = collect_crate_root_modules(&borrowed);
    let workspace_module_paths = collect_workspace_module_paths(&borrowed);
    let cfg_test = HashSet::new();
    let workspace = WorkspaceLookup {
        cfg_test_files: &cfg_test,
        crate_root_modules: &crate_root_modules,
        workspace_module_paths: &workspace_module_paths,
    };
    collect_private_candidates(
        &borrowed,
        &ws.aliases_per_file,
        &three_layer(),
        &HashSet::new(),
        &workspace,
    )
}

fn has_candidate(cands: &[PrivateCandidate], fn_name: &str) -> bool {
    cands.iter().any(|c| c.fn_name == fn_name)
}

#[test]
fn free_private_attributed_fn_is_recorded() {
    // A file-root private fn carrying a non-stdlib attribute is exactly
    // what a hint points at. Pins `visit_item_fn` against `-> ()` (which
    // would never call `record_if_candidate`) and the `attr_names`
    // projection.
    let cands = candidates_of("src/application/h.rs", "#[handler]\nfn make_widget() {}");
    assert!(
        has_candidate(&cands, "make_widget"),
        "free attributed fn collected: {cands:?}"
    );
    let c = cands
        .iter()
        .find(|c| c.fn_name == "make_widget")
        .expect("present");
    assert_eq!(c.attr_names, vec!["handler".to_string()], "attr recorded");
}

#[test]
fn fn_in_pub_inline_mod_is_recorded() {
    // The collector descends into a `pub mod` and records candidates
    // inside it. Pins `visit_item_mod` against `-> ()` (which would stop
    // the walk at the module boundary, hiding `nested_fn`).
    let cands = candidates_of(
        "src/application/h.rs",
        "pub mod inner { #[handler] fn nested_fn() {} }",
    );
    assert!(
        has_candidate(&cands, "nested_fn"),
        "fn inside pub mod collected: {cands:?}"
    );
}

#[test]
fn fn_in_private_inline_mod_is_not_recorded() {
    // A private inline `mod` is not visible, so a hint pointing inside it
    // would never resolve after promotion — the fn must be skipped while a
    // sibling top-level fn is still collected. Pins the
    // `parent_visible && is_visible(vis)` mod-visibility update against
    // `||` (which would keep the buried fn visible).
    let cands = candidates_of(
        "src/application/h.rs",
        "#[handler] fn top_fn() {}\nmod inner { #[handler] fn buried_fn() {} }",
    );
    assert!(
        has_candidate(&cands, "top_fn"),
        "control present: {cands:?}"
    );
    assert!(
        !has_candidate(&cands, "buried_fn"),
        "fn in private mod skipped: {cands:?}"
    );
}

#[test]
fn fn_in_invisible_impl_is_not_recorded() {
    // A method on a private (non-visible) type sits behind an invisible
    // impl self-type; promotion can't reach it. Pins both the
    // `!impl_stack.is_empty() && !current_impl_visible()` skip (the `!`)
    // and the `!segs.is_empty() && visible_canonicals.contains(..)`
    // impl-visibility `&&` — either mutation would surface `method_fn`.
    let cands = candidates_of(
        "src/application/h.rs",
        "struct Hidden;\nimpl Hidden { #[handler] fn method_fn() {} }\n#[handler] fn free_fn() {}",
    );
    assert!(
        has_candidate(&cands, "free_fn"),
        "control present: {cands:?}"
    );
    assert!(
        !has_candidate(&cands, "method_fn"),
        "method on private type skipped: {cands:?}"
    );
}

#[test]
fn test_attributed_fn_is_not_recorded() {
    // A `#[test]` fn disappears from the call graph, so it must never be a
    // hint candidate even when it also carries a promotable attribute.
    // Pins the `has_cfg_test(attrs) || has_test_attr(attrs)` skip against
    // `&&` (which would require BOTH, letting the `#[test]`-only fn pass).
    let cands = candidates_of(
        "src/application/h.rs",
        "#[test]\n#[handler]\nfn test_like() {}\n#[handler]\nfn real() {}",
    );
    assert!(has_candidate(&cands, "real"), "control present: {cands:?}");
    assert!(
        !has_candidate(&cands, "test_like"),
        "#[test] fn skipped despite handler attr: {cands:?}"
    );
}

fn candidate(file: &str, fn_name: &str) -> PrivateCandidate {
    PrivateCandidate {
        canonical: format!("crate::cli::{fn_name}"),
        file: file.to_string(),
        line: 1,
        fn_name: fn_name.to_string(),
        layer: Some("cli".to_string()),
        attr_names: vec!["handler".to_string()],
    }
}

#[test]
fn format_hint_uses_singular_vs_plural_per_count() {
    // One hit → "1 private fn …"; two hits → "2 private fns …". Pins the
    // `hits.len() == 1` branch against `!=` (which would swap the forms).
    let a = candidate("src/cli/a.rs", "alpha");
    let b = candidate("src/cli/b.rs", "beta");
    let singular = format_hint(&[("cli".to_string(), vec![&a])]);
    assert!(
        singular.contains("1 private fn in cli") && singular.contains("transitively reaches"),
        "singular noun + verb: {singular}"
    );
    let plural = format_hint(&[("cli".to_string(), vec![&a, &b])]);
    assert!(
        plural.contains("2 private fns in cli") && plural.contains("transitively reach this"),
        "plural noun + verb: {plural}"
    );
}
