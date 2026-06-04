use super::*;

const MISSING_ADAPTER_CASES: &[MissingCase] = &[
    (
        // REST only reaches the target through a layer-less intermediate
        // (`src/shared/` is not a mapped layer), so call_depth = 1 stops
        // the backward BFS at depth 1 while cli + mcp hit it directly.
        "rest reaches target only past call_depth → rest missing",
        &[
            ("src/application/stats.rs", "pub fn get_stats() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::stats::get_stats;
                    pub fn cmd_stats() { get_stats(); }
                    "#,
            ),
            (
                "src/mcp/handlers.rs",
                r#"
                    use crate::application::stats::get_stats;
                    pub fn handle_stats() { get_stats(); }
                    "#,
            ),
            (
                "src/shared/helpers.rs",
                r#"
                    use crate::application::stats::get_stats;
                    pub fn deep_call() { get_stats(); }
                    "#,
            ),
            (
                "src/rest/handlers.rs",
                r#"
                    use crate::shared::helpers::deep_call;
                    pub fn post_stats() { deep_call(); }
                    "#,
            ),
        ],
        1,
        &["cli", "mcp", "rest"],
        &[],
        "get_stats",
        &["rest"],
    ),
    (
        // `x.get_stats()` on an unknown-type receiver → `<method>:…`,
        // layer-unknown, so it doesn't count as reaching the target.
        "unknown-type method receiver does not count as reaching target",
        &[
            ("src/application/stats.rs", "pub fn get_stats() {}"),
            (
                "src/cli/handlers.rs",
                "pub fn cmd_stats(x: UnknownType) { x.get_stats(); }",
            ),
        ],
        3,
        &["cli"],
        &[],
        "get_stats",
        &["cli"],
    ),
    (
        "target outside the exclude glob is still checked → mcp missing",
        &[
            ("src/application/setup.rs", "pub fn run() {}"),
            ("src/application/stats.rs", "pub fn get_stats() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::setup::run;
                    use crate::application::stats::get_stats;
                    pub fn cmd_setup() { run(); }
                    pub fn cmd_stats() { get_stats(); }
                    "#,
            ),
        ],
        3,
        &["cli", "mcp"],
        &["application::setup::*"],
        "get_stats",
        &["mcp"],
    ),
    (
        "capability present in one adapter only fires for that target",
        &[
            (
                "src/application/session.rs",
                r#"
                    pub fn search() {}
                    pub fn admin_purge() {}
                    "#,
            ),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::session::{search, admin_purge};
                    pub fn cmd_search() { search(); }
                    pub fn cmd_admin() { admin_purge(); }
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
        3,
        &["cli", "mcp"],
        &[],
        "admin_purge",
        &["mcp"],
    ),
];

#[test]
fn check_b_reports_specific_missing_adapter() {
    for (label, files, depth, adapters, exclude, target_suffix, missing) in MISSING_ADAPTER_CASES {
        let cp = make_config(*depth, adapters, exclude);
        let pairs = missing_pairs(&run_b(&build_workspace(files), &cp));
        assert_eq!(pairs.len(), 1, "case {label}: {pairs:?}");
        assert!(
            pairs[0].0.ends_with(target_suffix),
            "case {label}: {pairs:?}"
        );
        let mut got = pairs[0].1.clone();
        got.sort();
        let mut exp: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
        exp.sort();
        assert_eq!(got, exp, "case {label}");
    }
}

#[test]
fn test_target_fn_only_called_from_tests_fails() {
    let ws = build_workspace(&[
        ("src/application/stats.rs", "pub fn get_stats() {}"),
        (
            "src/cli/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn cmd_stats() { get_stats(); }
            "#,
        ),
        (
            "src/mcp/handlers.rs",
            r#"
            use crate::application::stats::get_stats;
            pub fn handle_stats() { get_stats(); }
            "#,
        ),
        (
            "src/rest/tests.rs",
            r#"
            use crate::application::stats::get_stats;
            #[cfg(test)]
            mod tests {
                use super::*;
                #[test]
                fn test_stats() { get_stats(); }
            }
            "#,
        ),
    ]);
    let cp = make_config(3, &["cli", "mcp", "rest"], &[]);
    let mut cfg_test = HashSet::new();
    cfg_test.insert("src/rest/tests.rs".to_string());
    let findings = run_check_b(&ws, &four_layer(), &cp, &cfg_test);
    let pairs = missing_pairs(&findings);
    assert_eq!(pairs.len(), 1, "got {findings:?}");
    assert_eq!(pairs[0].1, vec!["rest".to_string()]);
}
