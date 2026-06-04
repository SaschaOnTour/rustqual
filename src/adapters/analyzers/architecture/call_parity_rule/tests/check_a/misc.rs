use super::*;

#[test]
fn test_adapter_fn_cfg_test_file_skipped() {
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            pub fn cmd_stats() {
                let _ = 42;
            }
            "#,
        ),
    ]);
    let mut cfg_test = std::collections::HashSet::new();
    cfg_test.insert("src/cli/handlers.rs".to_string());
    let findings = run_check_a(&ws, &three_layer(), &cli_mcp_config(3), &cfg_test);
    assert!(
        findings.is_empty(),
        "cfg-test adapter file must not produce findings, got {findings:?}"
    );
}

#[test]
fn test_adapter_fn_not_in_any_adapter_layer_ignored() {
    // Fn in a layer NOT listed as an adapter → not checked.
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/application/api.rs",
            r#"
            pub fn internal_api() {
                let _ = 42;
            }
            "#,
        ),
    ]);
    let findings = run_a(&ws, 3);
    assert!(
        findings.is_empty(),
        "non-adapter-layer fn must not be checked"
    );
}

#[test]
fn test_finding_line_is_fn_sig_line() {
    let src = "\n\n\npub fn cmd_stats() { let _ = 42; }\n";
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        ("src/cli/handlers.rs", src),
    ]);
    let findings = run_a(&ws, 3);
    let finding = findings
        .iter()
        .find(|f| matches!(f.kind, ViolationKind::CallParityNoDelegation { .. }))
        .expect("expected a CallParityNoDelegation finding");
    // `pub fn cmd_stats` is on line 4 (1-indexed) given 3 leading newlines.
    assert_eq!(
        finding.line, 4,
        "line must anchor on fn sig, got {finding:?}"
    );
    assert_eq!(finding.file, "src/cli/handlers.rs");
}
