use super::*;

// ── Orphan target-layer islands (v1.2.1) ───────────────────────

// Orphan target-layer islands (v1.2.1) + boundary semantic (v1.2.1): a target
// fn is flagged only when no adapter reaches it transitively (a dead island, or
// a self-only caller). A post-boundary internal helper that an adapter reaches
// through a boundary touchpoint — even asymmetrically — is application plumbing
// and must stay silent. (label, files, target_suffix, should_be_flagged)
const VISIBILITY_CASES: &[VisibilityCase] = &[
    (
        "orphan target with only a dead target-internal caller fires",
        &[
            (
                "src/application/admin.rs",
                r#"
                    pub fn admin_purge() {}
                    pub fn _legacy_wrapper() { admin_purge(); }
                    "#,
            ),
            ("src/cli/handlers.rs", "pub fn cmd_other() {}"),
            ("src/mcp/handlers.rs", "pub fn handle_other() {}"),
        ],
        "admin_purge",
        true,
    ),
    (
        "self-only caller orphan still fires",
        &[(
            "src/application/admin.rs",
            "pub fn admin_purge() { admin_purge(); }",
        )],
        "admin_purge",
        true,
    ),
    (
        "target reached transitively via target chain does not fire",
        &[
            (
                "src/application/middleware.rs",
                "pub fn record_operation() {}",
            ),
            (
                "src/application/session.rs",
                r#"
                    use crate::application::middleware::record_operation;
                    pub fn search() { record_operation(); }
                    "#,
            ),
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
                    use crate::application::session::search;
                    pub fn handle_search() { search(); }
                    "#,
            ),
        ],
        "record_operation",
        false,
    ),
    (
        "post-boundary helper silent under asymmetric transitive reach",
        &[
            (
                "src/application/middleware.rs",
                "pub fn record_operation() {}",
            ),
            (
                "src/application/session.rs",
                r#"
                    use crate::application::middleware::record_operation;
                    pub fn search() { record_operation(); }
                    pub fn admin() {}
                    "#,
            ),
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
                    use crate::application::session::admin;
                    pub fn handle_admin() { admin(); }
                    "#,
            ),
        ],
        "record_operation",
        false,
    ),
];

#[test]
fn check_b_orphan_and_boundary_target_visibility() {
    let cp = make_config(3, &["cli", "mcp"], &[]);
    for (label, files, target_suffix, flagged) in VISIBILITY_CASES {
        let findings = run_b(&build_workspace(files), &cp);
        let found = missing_pairs(&findings)
            .iter()
            .any(|(t, _)| t.ends_with(target_suffix));
        assert_eq!(found, *flagged, "case {label}: {findings:?}");
    }
}
