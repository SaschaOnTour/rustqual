//! Tests for the symbol/type collectors that feed the call-parity
//! pipeline: `collect_local_symbols` (item-name table), the workspace
//! type-canonical walker (`pub_fns_alias_chain`), and the call-graph
//! `FileFnCollector` test-attr skip. Each isolates one match arm /
//! cfg-test guard / `||`→`&&`.

use super::support::{build_graph_only, build_workspace, three_layer};
use crate::adapters::analyzers::architecture::call_parity_rule::file_visibility::collect_file_root_visibility;
use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::{
    collect_workspace_module_paths, WorkspaceLookup,
};
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::build_reexports_for_pub_fns;
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns_alias_chain::{
    collect_alias_chain, collect_workspace_type_canonicals,
};
use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns_visibility::collect_visible_type_canonicals_workspace;
use crate::adapters::analyzers::architecture::call_parity_rule::tests::collect_local_symbols;
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::collect_crate_root_modules;
use crate::adapters::shared::use_tree::{gather_alias_map, AliasMap};
use std::collections::{HashMap, HashSet};

fn parse(src: &str) -> syn::File {
    syn::parse_str(src).expect("parse")
}

// ── local_symbols::item_name table ──────────────────────────────────────

#[test]
fn collect_local_symbols_covers_every_item_kind() {
    // Each item kind contributes its declared name. Pins the
    // `Item::Enum/Union/Const/Static` arms of `item_name` against
    // deletion (which would drop that name).
    let ast = parse(
        "fn f() {}\nstruct S;\nenum E {}\nunion U { a: u8 }\ntrait T {}\ntype Ty = u8;\nconst C: u8 = 0;\nstatic St: u8 = 0;",
    );
    let syms = collect_local_symbols(&ast);
    for name in ["f", "S", "E", "U", "T", "Ty", "C", "St"] {
        assert!(syms.contains(name), "`{name}` collected: {syms:?}");
    }
}

// ── pub_fns_alias_chain::walk_type_canonicals ───────────────────────────

#[test]
fn workspace_type_canonicals_cover_kinds_and_skip_cfg_test_mod() {
    // struct/enum/union/trait/type each contribute a canonical; a type
    // inside a `#[cfg(test)] mod` is excluded. Pins the
    // `Enum/Union/Trait` arms against deletion and the
    // `!has_cfg_test(&m.attrs)` mod guard against `true`.
    let src = "pub struct S; pub enum E {} pub union U { a: u8 } pub trait T {} pub type Ty = u8;\n#[cfg(test)] mod tests { pub struct Hidden; }";
    let ast = parse(src);
    let files = vec![("src/app/a.rs", &ast)];
    let cfg_test = HashSet::new();
    let canon = collect_workspace_type_canonicals(&files, &cfg_test);
    for leaf in ["::S", "::E", "::U", "::T", "::Ty"] {
        assert!(
            canon.iter().any(|c| c.ends_with(leaf)),
            "canonical ending {leaf} present: {canon:?}"
        );
    }
    assert!(
        !canon.iter().any(|c| c.ends_with("::Hidden")),
        "type inside #[cfg(test)] mod excluded: {canon:?}"
    );
}

// ── file_fn_collector test-attr skip ────────────────────────────────────

#[test]
fn graph_excludes_test_fns_as_callers() {
    // A `#[test]` fn that calls a production fn must NOT become a graph
    // caller node — pins `visit_item_fn`'s `has_cfg_test || has_test_attr`
    // skip against `&&` (which would record the test fn and its edge).
    let ws = build_workspace(&[(
        "src/application/a.rs",
        "pub fn prod() {}\n#[test]\nfn t_caller() { prod(); }",
    )]);
    let graph = build_graph_only(&ws, &three_layer(), &HashSet::new(), &HashSet::new());
    assert!(
        !graph.forward.keys().any(|k| k.ends_with("::t_caller")),
        "test fn must not be a caller node: {:?}",
        graph.forward.keys().collect::<Vec<_>>()
    );
}

#[test]
fn graph_excludes_test_impl_methods_as_callers() {
    // Same skip for impl methods — pins `visit_impl_item_fn`'s
    // `has_cfg_test || has_test_attr` against `&&`.
    let ws = build_workspace(&[(
        "src/application/a.rs",
        "pub fn prod() {}\npub struct S;\nimpl S { #[test] fn t_method(&self) { prod(); } }",
    )]);
    let graph = build_graph_only(&ws, &three_layer(), &HashSet::new(), &HashSet::new());
    assert!(
        !graph.forward.keys().any(|k| k.ends_with("::t_method")),
        "test impl method must not be a caller node: {:?}",
        graph.forward.keys().collect::<Vec<_>>()
    );
}

// ── collect_visible_type_canonicals_workspace (is_visible guards) ────────

fn workspace_lookup<'a>(
    crate_roots: &'a HashSet<String>,
    module_paths: &'a HashSet<Vec<String>>,
    cfg_test: &'a HashSet<String>,
) -> WorkspaceLookup<'a> {
    WorkspaceLookup {
        cfg_test_files: cfg_test,
        crate_root_modules: crate_roots,
        workspace_module_paths: module_paths,
    }
}

#[test]
fn visible_type_canonicals_include_public_decls_only() {
    // Public struct/enum/union/trait are visible; private ones are not.
    // Pins the `is_visible(&_.vis)` guards on the Enum/Union/Trait arms of
    // `collect_in_items` against `true`/`false`.
    let ast = parse(
        "pub enum PubE {} enum PrivE {} pub union PubU { a: u8 } union PrivU { a: u8 } pub trait PubT {} trait PrivT {} pub type PubTy = u8; type PrivTy = u8;",
    );
    let files = vec![("src/app/a.rs", &ast)];
    let aliases: HashMap<String, AliasMap> = HashMap::new();
    let roots = collect_crate_root_modules(&files);
    let module_paths = collect_workspace_module_paths(&files);
    let cfg_test = HashSet::new();
    let workspace = workspace_lookup(&roots, &module_paths, &cfg_test);
    let visible =
        collect_visible_type_canonicals_workspace(&files, &aliases, &workspace, &HashSet::new());
    for leaf in ["::PubE", "::PubU", "::PubT", "::PubTy"] {
        assert!(
            visible.iter().any(|c| c.ends_with(leaf)),
            "public {leaf} visible: {visible:?}"
        );
    }
    // Each private decl must be EXCLUDED — pins every `is_visible(&_.vis)`
    // guard (enum/union/trait/type) against `-> true`.
    for leaf in ["::PrivE", "::PrivU", "::PrivT", "::PrivTy"] {
        assert!(
            !visible.iter().any(|c| c.ends_with(leaf)),
            "private {leaf} not visible: {visible:?}"
        );
    }
}

// ── collect_alias_chain (cfg-test mod guard) ────────────────────────────

#[test]
fn alias_chain_skips_aliases_inside_cfg_test_mod() {
    // A `type` alias inside a `#[cfg(test)] mod` is excluded from the
    // alias chain; a top-level one is kept. Pins `walk_alias_chain`'s
    // `!has_cfg_test(&m.attrs)` mod guard against `true`.
    let src = "pub struct Real;\npub type TopAlias = Real;\n#[cfg(test)] mod tests { pub type HiddenAlias = super::Real; }";
    let ast = parse(src);
    let files = vec![("src/app/a.rs", &ast)];
    let mut aliases: HashMap<String, AliasMap> = HashMap::new();
    aliases.insert("src/app/a.rs".to_string(), gather_alias_map(&ast));
    let roots = collect_crate_root_modules(&files);
    let module_paths = collect_workspace_module_paths(&files);
    let cfg_test = HashSet::new();
    let workspace = workspace_lookup(&roots, &module_paths, &cfg_test);
    let chain = collect_alias_chain(&files, &aliases, &workspace, &HashSet::new());
    assert!(
        chain.keys().any(|k| k.ends_with("::TopAlias")),
        "top-level alias collected: {chain:?}"
    );
    assert!(
        !chain.keys().any(|k| k.ends_with("::HiddenAlias")),
        "alias inside #[cfg(test)] mod excluded: {chain:?}"
    );
}

// ── file_visibility::descend_into_mod (via collect_file_root_visibility) ──

#[test]
fn file_root_visibility_descends_the_module_chain_to_nested_files() {
    // `src/a/sub.rs` is reachable via `mod a; mod sub;` and so is visible.
    // Reaching it requires `descend_into_mod` to recurse into `a` and then
    // resolve the nested `sub` — pins it against `-> Some(false)` (would
    // mark the nested file invisible) and `-> None` (which flattens to a
    // non-visible result for the deeper path).
    let lib = parse("pub mod a;");
    let a = parse("pub mod sub;");
    let sub = parse("pub fn f() {}");
    let files = vec![
        ("src/lib.rs", &lib),
        ("src/a.rs", &a),
        ("src/a/sub.rs", &sub),
    ];
    let vis = collect_file_root_visibility(&files);
    assert_eq!(
        vis.get("src/a/sub.rs"),
        Some(&true),
        "nested module file resolved through the chain: {vis:?}"
    );
}

// ── pub_fns::build_reexports_for_pub_fns (cfg-test filter) ───────────────

#[test]
fn build_reexports_resolves_non_test_file_pub_uses() {
    // A bare `pub use Widget as W;` in a non-test file resolves `Widget`
    // through THAT file's local symbols to `crate::app::prod::Widget`. The
    // `!cfg_test_files.contains` filters keep the non-test file in the
    // symbol/alias context; deleting a `!` keeps ONLY the test file, so
    // `Widget` no longer resolves and the re-export target changes. Assert
    // the resolved TARGET, not just the key.
    let prod = parse("pub struct Widget;\npub use Widget as W;");
    let testf = parse("pub fn helper() {}");
    let files = vec![("src/app/prod.rs", &prod), ("src/app/tests.rs", &testf)];
    let aliases: HashMap<String, AliasMap> = files
        .iter()
        .map(|(p, ast)| (p.to_string(), gather_alias_map(ast)))
        .collect();
    let roots = collect_crate_root_modules(&files);
    let module_paths = collect_workspace_module_paths(&files);
    let cfg_test: HashSet<String> = HashSet::from(["src/app/tests.rs".to_string()]);
    let workspace = workspace_lookup(&roots, &module_paths, &cfg_test);
    let reexports = build_reexports_for_pub_fns(&files, &aliases, &workspace);
    assert!(
        reexports
            .iter()
            .any(|(k, v)| k.ends_with("::W") && v.ends_with("::Widget")),
        "W re-export resolves to the local Widget: {reexports:?}"
    );
}
