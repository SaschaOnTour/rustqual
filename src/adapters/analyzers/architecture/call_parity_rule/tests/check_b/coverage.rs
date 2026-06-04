use super::*;

// ── Direct / transitive coverage ──────────────────────────────

#[test]
fn target_fn_adapter_coverage() {
    // (label, adapters that call get_stats, expected missing adapters)
    let cases: &[(&str, &[&str], &[&str])] = &[
        (
            "called from all adapters → covered",
            &["cli", "mcp", "rest"],
            &[],
        ),
        ("missing one adapter", &["cli", "mcp"], &["rest"]),
        ("missing two adapters", &["cli"], &["mcp", "rest"]),
        ("not called from any adapter", &[], &["cli", "mcp", "rest"]),
    ];
    let cp = make_config(3, &["cli", "mcp", "rest"], &[]);
    for (label, present, expected_missing) in cases {
        let pairs = missing_pairs(&run_b(&stats_ws(present), &cp));
        if expected_missing.is_empty() {
            assert!(
                pairs.is_empty(),
                "case {label}: expected coverage, got {pairs:?}"
            );
        } else {
            assert_eq!(pairs.len(), 1, "case {label}: {pairs:?}");
            assert!(pairs[0].0.ends_with("get_stats"), "case {label}");
            let mut missing = pairs[0].1.clone();
            missing.sort();
            let mut exp: Vec<String> = expected_missing.iter().map(|s| s.to_string()).collect();
            exp.sort();
            assert_eq!(missing, exp, "case {label}");
        }
    }
}

// Cases where Check B must produce NO missing-adapter finding: the target is
// reached by every configured adapter (directly, via a service wrapper, or via
// receiver-tracked / cross-file impl calls), is private (not on the parity
// surface), or is excluded by glob. (label, files, adapters, exclude_glob)
const SILENT_CASES: &[SilentCase] = &[
    (
        "rest reaches the target via a service wrapper (depth 2)",
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
                "src/rest/service.rs",
                r#"
                    use crate::application::stats::get_stats;
                    pub fn wrap() { get_stats(); }
                    "#,
            ),
            (
                "src/rest/handlers.rs",
                r#"
                    use crate::rest::service::wrap;
                    pub fn post_stats() { wrap(); }
                    "#,
            ),
        ],
        &["cli", "mcp", "rest"],
        &[],
    ),
    (
        "private target fn is not on the parity surface",
        &[("src/application/stats.rs", "fn get_stats() {}")],
        &["cli", "mcp", "rest"],
        &[],
    ),
    (
        "method call via receiver binding counts as reaching the target",
        &[
            (
                "src/application/session.rs",
                r#"
                    pub struct Session;
                    impl Session {
                        pub fn open() -> Self { Session }
                        pub fn search(&self) {}
                    }
                    "#,
            ),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::session::Session;
                    pub fn cmd_search() {
                        let s = Session::open();
                        s.search();
                    }
                    "#,
            ),
            (
                "src/mcp/handlers.rs",
                r#"
                    use crate::application::session::Session;
                    pub fn handle_search() {
                        let s = Session::open();
                        s.search();
                    }
                    "#,
            ),
        ],
        &["cli", "mcp"],
        &[],
    ),
    (
        "target matched by exclude_targets glob is ignored",
        &[
            ("src/application/setup.rs", "pub fn run() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::setup::run;
                    pub fn cmd_setup() { run(); }
                    "#,
            ),
        ],
        &["cli", "mcp", "rest"],
        &["application::setup::*"],
    ),
    (
        "exclude glob uses canonical without the crate:: prefix",
        &[("src/application/setup.rs", "pub fn run() {}")],
        &["cli", "mcp"],
        &["application::setup::run"],
    ),
    (
        "deeper application-internal chain fires nothing",
        &[
            (
                "src/application/middleware.rs",
                r#"
                    pub fn impact_count() -> u32 { 0 }
                    pub fn record_operation() { impact_count(); }
                    "#,
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
        &["cli", "mcp"],
        &[],
    ),
    (
        "cross-file impl via use matches receiver-tracked calls",
        &[
            ("src/application/session.rs", "pub struct Session;"),
            (
                "src/application/session_impls.rs",
                r#"
                    use crate::application::session::Session;
                    impl Session {
                        pub fn open() -> Self { Session }
                        pub fn search(&self) {}
                    }
                    "#,
            ),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::session::Session;
                    pub fn cmd_search() {
                        let s = Session::open();
                        s.search();
                    }
                    "#,
            ),
            (
                "src/mcp/handlers.rs",
                r#"
                    use crate::application::session::Session;
                    pub fn handle_search() {
                        let s = Session::open();
                        s.search();
                    }
                    "#,
            ),
        ],
        &["cli", "mcp"],
        &[],
    ),
];

#[test]
fn check_b_silent_when_target_fully_covered_or_out_of_scope() {
    for (label, files, adapters, exclude) in SILENT_CASES {
        let cp = make_config(3, adapters, exclude);
        let findings = run_b(&build_workspace(files), &cp);
        assert!(
            missing_pairs(&findings).is_empty(),
            "case {label}: expected no findings, got {findings:?}"
        );
    }
}

// Cases where exactly one target is under-covered: a single finding names the
// target and the adapter(s) that fail to reach it. Covers shallow call_depth,
// unresolved method receivers, a partial exclude glob, and a capability present
// in one adapter only.
// (label, files, depth, adapters, exclude_glob, target_suffix, missing)
