use super::*;

#[test]
fn check_b_does_not_skip_concrete_when_an_adapter_calls_it_directly() {
    // Mixed-form drift: cli direct concrete, mcp dispatch. The
    // concrete pass must still run so the form-mismatch surfaces.
    let pairs = missing_pairs(&run_b_ports(&[
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
            pub fn cmd_log() {
                LoggingHandler::handle(&LoggingHandler);
            }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::ports::handler::Handler;
            pub fn mcp_dispatch(h: &dyn Handler) { h.handle(); }
            "#,
        ),
    ]));
    let concrete = "crate::application::logging::LoggingHandler::handle";
    let concrete_finding = pairs.iter().find(|(t, _)| t == concrete);
    assert!(
        concrete_finding.is_some(),
        "concrete impl-method must NOT be silently skipped when at least one adapter (cli) calls it directly — drift between cli (direct) and mcp (dispatch) must surface; got {pairs:?}"
    );
    if let Some((_, missing)) = concrete_finding {
        assert!(
            missing.iter().any(|a| a == "mcp"),
            "concrete pass must report mcp as missing (it reaches via dispatch, not direct concrete); got {missing:?}"
        );
    }
}

#[test]
fn anchor_finding_carries_trait_method_source_line() {
    // Anchor findings must carry the trait method's real file + line
    // (1-based) — line=0 placeholders break suppression-window matching,
    // the orphan detector's window scan, and SARIF startLine validity.
    let findings = run_b_ports(&[
        (
            // Lines: 1=blank, 2=trait header, 3=method decl
            "src/ports/orphan.rs",
            "\npub trait Orphan {\n    fn handle(&self);\n}\n",
        ),
        (
            "src/application/orphan_impl.rs",
            r#"
            use crate::ports::orphan::Orphan;
            pub struct OrphanImpl;
            impl Orphan for OrphanImpl { fn handle(&self) {} }
            "#,
        ),
        ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
        ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
    ]);
    let anchor = "crate::ports::orphan::Orphan::handle";
    let hit = findings
        .iter()
        .find(|f| match &f.kind {
            ViolationKind::CallParityMissingAdapter { target_fn, .. } => target_fn == anchor,
            _ => false,
        })
        .unwrap_or_else(|| panic!("anchor orphan finding missing, got {findings:?}"));
    assert_eq!(
        hit.file, "src/ports/orphan.rs",
        "anchor finding must carry the trait method's source file, got {hit:?}"
    );
    assert_eq!(
        hit.line, 3,
        "anchor finding must carry the trait method's 1-based line number, got {hit:?}"
    );
    // F6.3 cross-check: anchor finding line is non-zero AND a
    // valid 1-based line number, matching SARIF startLine
    // requirements and what suppression-window matchers expect.
    assert!(
        hit.line >= 1,
        "anchor finding line must be 1-based (>=1), got {}",
        hit.line
    );
}

#[test]
fn check_b_anchor_only_target_surface_still_inspected() {
    // Target layer has NO concrete pub fns — only a default-body trait
    // declared there. The trait method is a target capability via the
    // unified rule. Even though in practice `pub_fns_by_layer["application"]`
    // is created (empty vec) by the collector's `or_default()`, the
    // target-anchor branch must still fire a missing-adapter finding.
    // Sister test below directly exercises the `None` branch via a
    // hand-stripped pub_fns_by_layer.
    let ws = build_workspace(&[
        (
            "src/application/cap.rs",
            "pub trait Cap { fn run(&self) {} }",
        ),
        ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
        ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
    ]);
    let findings = run_check_b(&ws, &ports_app_cli_mcp(), &ports_cp(), &empty_cfg_test());
    let pairs = missing_pairs(&findings);
    let anchor = "crate::application::cap::Cap::run";
    assert!(
        pairs.iter().any(|(target, _)| target == anchor),
        "anchor in anchor-only target layer must still fire missing-adapter, got {pairs:?}"
    );
}

#[test]
fn check_b_anchor_inspected_even_when_target_layer_absent_from_pub_fns_map() {
    // Strips the target entry from pub_fns_by_layer to exercise the
    // `None` branch of `get(target)`. Anchor enumeration must still
    // run.
    use crate::adapters::analyzers::architecture::call_parity_rule::build_handler_touchpoints;
    use crate::adapters::analyzers::architecture::call_parity_rule::check_b::check_missing_adapter;
    use crate::adapters::analyzers::architecture::call_parity_rule::pub_fns::{
        collect_pub_fns_by_layer, PubFnInputs,
    };
    use crate::adapters::analyzers::architecture::call_parity_rule::workspace_graph::build_call_graph;

    let ws = build_workspace(&[
        (
            "src/application/cap.rs",
            "pub trait Cap { fn run(&self) {} }",
        ),
        ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
        ("src/mcp/handlers.rs", "pub fn mcp_other() {}"),
    ]);
    let layers = ports_app_cli_mcp();
    let cp = ports_cp();
    let cfg_test = empty_cfg_test();
    let borrowed = borrowed_files(&ws);
    let crate_root_modules = HashSet::new();
    let workspace_module_paths = HashSet::new();
    let workspace = crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::WorkspaceLookup {
        cfg_test_files: &cfg_test,
        crate_root_modules: &crate_root_modules,
        workspace_module_paths: &workspace_module_paths,
    };
    let mut pub_fns = collect_pub_fns_by_layer(PubFnInputs {
        files: &borrowed,
        aliases_per_file: &ws.aliases_per_file,
        layers: &layers,
        transparent_wrappers: &cp.transparent_wrappers,
        promoted_attributes: &cp.promoted_attributes,
        workspace: &workspace,
    });
    let graph = build_call_graph(
        &borrowed,
        &ws.aliases_per_file,
        &layers,
        &cp.transparent_wrappers,
        &workspace,
    );
    let touchpoints = build_handler_touchpoints(&pub_fns, &graph, &cp);
    pub_fns.remove("application");
    assert!(
        !pub_fns.contains_key("application"),
        "test precondition: target layer key must be absent before invoking check_missing_adapter"
    );
    let findings = check_missing_adapter(&pub_fns, &graph, &touchpoints, &cp);
    let pairs = missing_pairs(&findings);
    let anchor = "crate::application::cap::Cap::run";
    assert!(
        pairs.iter().any(|(target, _)| target == anchor),
        "with target layer absent from pub_fns_by_layer, anchor enumeration must still run; got {pairs:?}"
    );
}
