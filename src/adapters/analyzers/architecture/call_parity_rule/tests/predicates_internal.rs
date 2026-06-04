//! Unit tests for small pure predicates across the top-level
//! call-parity modules: the Check B/D adapter-coverage probes, the
//! reachability `is_target_capability_node`, the `project_call_parity` /
//! `severity_for` projections, `param_name_from_pat`, and the
//! binding-init / alias-normalisation peels.

use crate::adapters::analyzers::architecture::call_parity_rule::anchor_index::AnchorInfo;
use crate::adapters::analyzers::architecture::call_parity_rule::bindings::{
    binding_type_from_init, normalize_after_alias, CanonScope,
};
use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::CallGraph;
use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};
use crate::adapters::shared::use_tree::{AliasMap, ScopedAliasMap};
use crate::config::architecture::SingleTouchpointMode;
use crate::domain::Severity;
use std::collections::{HashMap, HashSet};

// ── check_b / check_d adapter-coverage probes ───────────────────────────

#[test]
fn any_adapter_reaches_concrete_requires_a_real_hit() {
    use crate::adapters::analyzers::architecture::call_parity_rule::check_b::any_adapter_reaches_concrete;
    let mut coverage: HashMap<String, HashSet<String>> = HashMap::new();
    coverage.insert(
        "cli".into(),
        HashSet::from(["crate::app::handle".to_string()]),
    );
    assert!(any_adapter_reaches_concrete(
        "crate::app::handle",
        &coverage
    ));
    // No adapter covers it → false (pins `-> true`).
    assert!(!any_adapter_reaches_concrete(
        "crate::app::other",
        &coverage
    ));
}

#[test]
fn any_adapter_counts_concrete_requires_a_real_count() {
    use crate::adapters::analyzers::architecture::call_parity_rule::check_d::any_adapter_counts_concrete;
    let mut counts: HashMap<String, HashMap<String, usize>> = HashMap::new();
    counts.insert(
        "cli".into(),
        HashMap::from([("crate::app::h".to_string(), 1)]),
    );
    assert!(any_adapter_counts_concrete("crate::app::h", &counts));
    assert!(!any_adapter_counts_concrete("crate::app::missing", &counts));
}

#[test]
fn any_impl_canonical_covered_or_reachable_is_a_disjunction() {
    use crate::adapters::analyzers::architecture::call_parity_rule::check_b::any_impl_canonical_covered_or_reachable;
    let info = AnchorInfo {
        impl_layers: HashSet::new(),
        impl_method_canonicals: HashSet::from(["crate::app::Impl::m".to_string()]),
        decl_layer: None,
        has_default_body: false,
        trait_visible: true,
        location: None,
    };
    let empty_cov: HashMap<String, HashSet<String>> = HashMap::new();
    // Reachable but NOT covered → true via the `||` left operand. Pins
    // `||`→`&&` (which would also require coverage).
    let reachable = HashSet::from(["crate::app::Impl::m".to_string()]);
    assert!(any_impl_canonical_covered_or_reachable(
        &info, &empty_cov, &reachable
    ));
    // Neither reachable nor covered → false.
    assert!(!any_impl_canonical_covered_or_reachable(
        &info,
        &empty_cov,
        &HashSet::new()
    ));
}

// ── check_b_coverage::is_target_capability_node ─────────────────────────

fn graph_with_layer(node: &str, layer: &str, in_forward: bool) -> CallGraph {
    let mut layer_of = HashMap::new();
    layer_of.insert(node.to_string(), Some(layer.to_string()));
    let mut forward = HashMap::new();
    if in_forward {
        forward.insert(node.to_string(), HashSet::new());
    }
    CallGraph {
        forward,
        reverse: HashMap::new(),
        layer_of,
        trait_method_anchors: HashMap::new(),
    }
}

#[test]
fn is_target_capability_node_needs_layer_and_a_forward_entry() {
    use crate::adapters::analyzers::architecture::call_parity_rule::check_b_coverage::is_target_capability_node;
    // Target layer + present in `forward` → a real capability node.
    let real = graph_with_layer("crate::app::X", "app", true);
    assert!(is_target_capability_node(
        "crate::app::X",
        &real,
        "app",
        &[]
    ));
    // Same layer but NO forward entry (phantom sink) and no anchor →
    // false. Pins the `layer == target && forward.contains_key` against
    // `||` and the whole fn against `-> true`.
    let phantom = graph_with_layer("crate::app::X", "app", false);
    assert!(!is_target_capability_node(
        "crate::app::X",
        &phantom,
        "app",
        &[]
    ));
}

// ── mod::project_call_parity / severity_for ─────────────────────────────

fn loc(kind: ViolationKind) -> MatchLocation {
    MatchLocation {
        file: "src/cli/a.rs".into(),
        line: 3,
        column: 0,
        kind,
    }
}

fn multi_touchpoint() -> ViolationKind {
    ViolationKind::CallParityMultiTouchpoint {
        fn_name: "cmd".into(),
        adapter_layer: "cli".into(),
        touchpoints: vec!["crate::app::a".into(), "crate::app::b".into()],
    }
}

fn multiplicity_mismatch() -> ViolationKind {
    ViolationKind::CallParityMultiplicityMismatch {
        target_fn: "crate::app::search".into(),
        target_layer: "app".into(),
        counts_per_adapter: vec![("cli".into(), 2), ("mcp".into(), 1)],
    }
}

fn cp_with(
    mode: SingleTouchpointMode,
) -> crate::adapters::analyzers::architecture::compiled::CompiledCallParity {
    let mut cp = super::support::cli_mcp_config(2);
    cp.single_touchpoint = mode;
    cp
}

#[test]
fn project_call_parity_maps_each_kind_to_its_rule_id() {
    use crate::adapters::analyzers::architecture::call_parity_rule::project_call_parity;
    let cp = cp_with(SingleTouchpointMode::Error);
    let mm = project_call_parity(loc(multiplicity_mismatch()), &cp);
    assert_eq!(
        mm.rule_id, "architecture/call_parity/multiplicity_mismatch",
        "multiplicity arm"
    );
    let mt = project_call_parity(loc(multi_touchpoint()), &cp);
    assert_eq!(
        mt.rule_id, "architecture/call_parity/multi_touchpoint",
        "multi-touchpoint arm"
    );
}

#[test]
fn severity_for_multi_touchpoint_follows_single_touchpoint_mode() {
    use crate::adapters::analyzers::architecture::call_parity_rule::severity_for;
    // Error → Medium; Warn → Low. Pins the MultiTouchpoint arm against
    // deletion (which would fall through to the `_ => Medium` default).
    assert_eq!(
        severity_for(&multi_touchpoint(), &cp_with(SingleTouchpointMode::Error)),
        Severity::Medium
    );
    assert_eq!(
        severity_for(&multi_touchpoint(), &cp_with(SingleTouchpointMode::Warn)),
        Severity::Low,
        "Warn → Low pins the arm vs the Medium default"
    );
}

// ── signature_params::param_name_from_pat ───────────────────────────────

fn pat(src: &str) -> syn::Pat {
    let file: syn::File =
        syn::parse_str(&format!("fn _t() {{ let {src} = _x; }}")).expect("parse pat");
    match &file.items[0] {
        syn::Item::Fn(f) => match &f.block.stmts[0] {
            syn::Stmt::Local(local) => local.pat.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

#[test]
fn param_name_from_pat_handles_single_element_extractors_only() {
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::param_name_from_pat;
    // `State(db)` (one-element tuple-struct extractor) → `db`.
    assert_eq!(
        param_name_from_pat(&pat("State(db)")).as_deref(),
        Some("db")
    );
    // A two-element tuple struct is NOT a single-arg extractor → None.
    // Pins the `ts.elems.len() == 1` guard against `false`/`true`/`!=`.
    assert_eq!(param_name_from_pat(&pat("State(a, b)")), None);
}

// ── bindings::binding_type_from_init ────────────────────────────────────

fn expr(src: &str) -> syn::Expr {
    syn::parse_str(src).expect("parse expr")
}

#[test]
fn binding_type_from_init_peels_try_await_and_parens() {
    // The init peel loop unwraps `?`, `.await`, and parens before the
    // `Type::ctor()` lookup, so all four forms resolve identically. Pins
    // the `Expr::Try`/`Await`/`Paren` arms against deletion.
    let locals = HashSet::from(["Foo".to_string()]);
    let alias = AliasMap::new();
    let roots = HashSet::new();
    let call =
        |src: &str| binding_type_from_init(&expr(src), &alias, &locals, &roots, "src/app/x.rs");
    let base = call("Foo::new()");
    assert!(base.is_some(), "bare ctor resolves: {base:?}");
    assert_eq!(call("Foo::new()?"), base, "Try peeled");
    assert_eq!(call("Foo::new().await"), base, "Await peeled");
    assert_eq!(call("(Foo::new())"), base, "Paren peeled");
}

#[test]
fn binding_type_from_init_accepts_multi_segment_type_paths() {
    // A 3-segment `app::Foo::new()` splits to the 2-segment type prefix
    // `app::Foo`. Pins the `segments.len() < 2` guard against `>` (which
    // would reject the 3-segment path).
    let alias = AliasMap::new();
    let roots = HashSet::from(["app".to_string()]);
    let got = binding_type_from_init(
        &expr("app::Foo::new()"),
        &alias,
        &HashSet::new(),
        &roots,
        "src/app/x.rs",
    );
    assert!(got.is_some(), "multi-segment ctor resolves: {got:?}");
}

// ── shared minimal FileScope (empty maps) ───────────────────────────────

/// Owns the empty maps a `FileScope` borrows, so a single `.file_scope()`
/// call replaces the repeated 5-statement construction (DRY-005).
#[derive(Default)]
struct EmptyScope {
    alias_map: AliasMap,
    aliases_per_scope: ScopedAliasMap,
    local_symbols: HashSet<String>,
    local_decl_scopes: HashMap<String, Vec<Vec<String>>>,
    crate_roots: HashSet<String>,
}

impl EmptyScope {
    fn file_scope(&self) -> FileScope<'_> {
        FileScope {
            path: "src/app/x.rs",
            alias_map: &self.alias_map,
            aliases_per_scope: &self.aliases_per_scope,
            local_symbols: &self.local_symbols,
            local_decl_scopes: &self.local_decl_scopes,
            crate_root_modules: &self.crate_roots,
            workspace_module_paths: None,
        }
    }
}

// ── bindings::normalize_after_alias ─────────────────────────────────────

#[test]
fn normalize_after_alias_keeps_crate_rooted_paths() {
    // A `crate::…` expansion is returned as-is. Pins the `Some("crate")`
    // arm against deletion (which would fall through to the submodule /
    // crate-root heuristics and mis-handle it).
    let scope_owner = EmptyScope::default();
    let file = scope_owner.file_scope();
    let scope = CanonScope {
        file: &file,
        mod_stack: &[],
        reexports: None,
    };
    let got = normalize_after_alias(
        vec!["crate".to_string(), "app".to_string(), "Foo".to_string()],
        false,
        &scope,
    );
    assert_eq!(
        got,
        Some(vec![
            "crate".to_string(),
            "app".to_string(),
            "Foo".to_string()
        ]),
        "crate-rooted path passes through unchanged"
    );
}

// ── signature_params::method_canonical_generics (where-bound extending) ──

#[test]
fn method_where_bound_extends_an_impl_level_generic() {
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::method_canonical_generics;
    // A method `where Q: ExtraBound` for an impl-level generic `Q` must
    // merge that bound onto Q's entry. Pins `merge_where_bounds_extending`'s
    // `extending_names.iter().any(|n| *n == name)` against `!=` (which would
    // drop the where-bound, leaving Q with only its impl-level bound).
    let item: syn::ItemFn =
        syn::parse_str("fn m(&self) where Q: crate::Marker {}").expect("parse fn");
    let impl_generics = vec![(
        "Q".to_string(),
        vec![vec!["crate".to_string(), "Base".to_string()]],
    )];
    let scope_owner = EmptyScope::default();
    let file = scope_owner.file_scope();
    let out = method_canonical_generics(&item.sig, &impl_generics, &file, &[], None);
    let q = out.get("Q").expect("Q present");
    assert!(
        q.bounds
            .iter()
            .any(|b| b.last().is_some_and(|s| s == "Marker")),
        "where-clause Marker bound merged onto Q: {:?}",
        q.bounds
    );
}

#[test]
fn method_where_bound_extends_an_existing_method_generic() {
    use crate::adapters::analyzers::architecture::call_parity_rule::signature_params::method_canonical_generics;
    // A method generic `Q` with an inline bound AND a where-clause bound:
    // `merge_where_bounds_extending` must `find` Q's existing inline entry
    // and EXTEND it. Pins the `find(|(n, _)| n == &name)` lookup against
    // `!=` (which would miss Q's entry → fall through → drop the where-bound
    // since Q is not an outer/impl name).
    let item: syn::ItemFn =
        syn::parse_str("fn m<Q: crate::Base>(&self) where Q: crate::Extra {}").expect("parse fn");
    let scope_owner = EmptyScope::default();
    let file = scope_owner.file_scope();
    let out = method_canonical_generics(&item.sig, &[], &file, &[], None);
    let q = out.get("Q").expect("Q present");
    assert!(
        q.bounds
            .iter()
            .any(|b| b.last().is_some_and(|s| s == "Base")),
        "inline Base bound kept: {:?}",
        q.bounds
    );
    assert!(
        q.bounds
            .iter()
            .any(|b| b.last().is_some_and(|s| s == "Extra")),
        "where-clause Extra bound merged onto the existing Q entry: {:?}",
        q.bounds
    );
}
