use super::*;

// Check D fires only when a target IS reached by every adapter but the
// per-adapter handler counts diverge. Balanced fan-in, deprecated-alias
// exclusion, and a target missing entirely from one adapter (Check B's job)
// must all stay silent. (label, files, adapters, target_suffix, expected_counts)
const MULTIPLICITY_CASES: &[MultiplicityCase] = &[
    (
        // cli: cmd_search + cmd_grep → search; mcp: handle_search → search
        "asymmetric fan-in fires (cli=2, mcp=1)",
        &[
            ("src/application/session.rs", "pub fn search() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::session::search;
                    pub fn cmd_search() { search(); }
                    pub fn cmd_grep() { search(); }
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
        "session::search",
        &[("cli", 2), ("mcp", 1)],
    ),
    (
        "balanced fan-in is silent",
        &[
            ("src/application/session.rs", "pub fn search() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::session::search;
                    pub fn cmd_search() { search(); }
                    pub fn cmd_grep() { search(); }
                    "#,
            ),
            (
                "src/mcp/handlers.rs",
                r#"
                    use crate::application::session::search;
                    pub fn handle_search() { search(); }
                    pub fn handle_grep() { search(); }
                    "#,
            ),
        ],
        &["cli", "mcp"],
        "",
        &[],
    ),
    (
        "three adapters, one diverges (cli=2, mcp=1, rest=2)",
        &[
            ("src/application/session.rs", "pub fn search() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::session::search;
                    pub fn cmd_search() { search(); }
                    pub fn cmd_grep() { search(); }
                    "#,
            ),
            (
                "src/mcp/handlers.rs",
                r#"
                    use crate::application::session::search;
                    pub fn handle_search() { search(); }
                    "#,
            ),
            (
                "src/rest/handlers.rs",
                r#"
                    use crate::application::session::search;
                    pub fn post_search() { search(); }
                    pub fn post_grep() { search(); }
                    "#,
            ),
        ],
        &["cli", "mcp", "rest"],
        "session::search",
        &[("cli", 2), ("mcp", 1), ("rest", 2)],
    ),
    (
        // cmd_grep is `#[deprecated]` → excluded; cli=1, mcp=1 → silent
        "deprecated alias is excluded from the count",
        &[
            ("src/application/session.rs", "pub fn search() {}"),
            (
                "src/cli/handlers.rs",
                r#"
                    use crate::application::session::search;
                    pub fn cmd_search() { search(); }
                    #[deprecated]
                    pub fn cmd_grep() { search(); }
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
        "",
        &[],
    ),
    (
        // mcp never reaches search → Check B's job, Check D stays silent
        "target missing entirely from an adapter is silent (Check B's job)",
        &[
            ("src/application/session.rs", "pub fn search() {}"),
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
                    // mcp doesn't touch search at all
                    pub fn handle_other() {}
                    "#,
            ),
        ],
        &["cli", "mcp"],
        "",
        &[],
    ),
];

#[test]
fn multiplicity_mismatch_detection() {
    for (label, files, adapters, target_suffix, expected) in MULTIPLICITY_CASES {
        let pairs = multiplicity_4l(files, adapters);
        if expected.is_empty() {
            assert!(
                pairs.is_empty(),
                "case {label}: expected silence, got {pairs:?}"
            );
        } else {
            assert_eq!(pairs.len(), 1, "case {label}: {pairs:?}");
            let (target, counts) = &pairs[0];
            assert!(
                target.ends_with(target_suffix),
                "case {label}: target {target} should end with {target_suffix}"
            );
            for (adapter, want) in *expected {
                assert_eq!(
                    count_for(counts, adapter),
                    Some(*want),
                    "case {label}: adapter {adapter} count"
                );
            }
        }
    }
}
