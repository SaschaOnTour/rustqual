use super::*;

const PUB_FN_CASES: &[PubFnCase] = &[
        // ── visibility on free / impl fns (single file) ──────────────
        (
            "private (no-modifier) fn is skipped; pub sibling kept",
            &[(
                "src/cli/handlers.rs",
                "fn helper() {} pub fn cmd_stats() {}",
            )],
            "cli",
            &[],
            &["cmd_stats"],
            &["helper"],
        ),
        (
            "pub(super) and pub(in path) are public enough",
            &[(
                "src/cli/handlers.rs",
                "pub(super) fn cmd_a() {} pub(in crate::cli) fn cmd_b() {}",
            )],
            "cli",
            &[],
            &["cmd_a", "cmd_b"],
            &[],
        ),
        (
            "pub impl method on a pub type kept; private method skipped",
            &[(
                "src/application/session.rs",
                "pub struct Session; impl Session { pub fn search(&self) {} fn helper(&self) {} }",
            )],
            "application",
            &[],
            &["search"],
            &["helper"],
        ),
        (
            "#[test] fn is skipped; pub sibling kept",
            &[(
                "src/cli/handlers.rs",
                "#[test] pub fn not_a_handler() {} pub fn cmd_stats() {}",
            )],
            "cli",
            &[],
            &["cmd_stats"],
            &["not_a_handler"],
        ),
        (
            "pub fn inside a private inline mod is not reachable",
            &[(
                "src/cli/handlers.rs",
                "mod private { pub fn helper() {} } pub fn visible_top() {}",
            )],
            "cli",
            &[],
            &["visible_top"],
            &["helper"],
        ),
        (
            "pub(self) is private-equivalent and skipped",
            &[(
                "src/cli/handlers.rs",
                "pub(self) fn helper() {} pub fn visible() {}",
            )],
            "cli",
            &[],
            &["visible"],
            &["helper"],
        ),
        (
            // Distinct types named `Session` (public mod vs private mod) key on
            // full canonical paths, so the private one's method can't leak.
            "short-name type collision does not leak the private-mod method",
            &[(
                "src/cli/handlers.rs",
                "pub mod api { pub struct Session; impl Session { pub fn run(&self) {} } } \
                 mod internal { pub struct Session; impl Session { pub fn cleanup(&self) {} } }",
            )],
            "cli",
            &[],
            &["run"],
            &["cleanup"],
        ),
        // ── module-tree reachability (multi-file) ────────────────────
        (
            // `pub struct` in one file, `impl` (via `use`) in another — both
            // resolve to the same canonical, so the impl methods are surface.
            "cross-file impl on a pub type is collected",
            &[
                ("src/application/session.rs", "pub struct Session;"),
                (
                    "src/application/session_impls.rs",
                    "use crate::application::session::Session; impl Session { pub fn search(&self) {} }",
                ),
            ],
            "application",
            &[],
            &["search"],
            &[],
        ),
        (
            "pub fn in a file-backed private module (`mod internal;`) is skipped",
            &[
                ("src/application/mod.rs", "mod internal;"),
                ("src/application/internal.rs", "pub fn helper() {}"),
            ],
            "application",
            &[],
            &[],
            &["helper"],
        ),
        (
            "pub fn in a file-backed public module (`pub mod internal;`) is kept",
            &[
                ("src/application/mod.rs", "pub mod internal;"),
                ("src/application/internal.rs", "pub fn helper() {}"),
            ],
            "application",
            &[],
            &["helper"],
            &[],
        ),
        (
            // lib.rs never declares `mod application;`, so the file is an orphan
            // not part of any module tree.
            "orphan file not declared in the crate root contributes nothing",
            &[
                ("src/lib.rs", "mod cli;"),
                ("src/cli/mod.rs", "pub fn cmd() {}"),
                ("src/application/mod.rs", "pub fn helper() {}"),
            ],
            "application",
            &[],
            &[],
            &["helper"],
        ),
        (
            // A private `mod internal;` ancestor hides the whole subtree even
            // though the direct parent of `deep` says `pub`.
            "private ancestor in the chain hides pub fns in deep descendants",
            &[
                ("src/application/mod.rs", "mod internal;"),
                ("src/application/internal/mod.rs", "pub mod deep;"),
                ("src/application/internal/deep.rs", "pub fn helper() {}"),
            ],
            "application",
            &[],
            &[],
            &["helper"],
        ),
        // ── user-configured transparent wrappers ─────────────────────
        (
            // External `axum::*` paths can't be canonicalised, so the
            // visibility pass falls back to last-segment matching for the
            // user-transparent `State` wrapper.
            "fully-qualified user wrapper peels via leaf segment",
            &[(
                "src/cli/handlers.rs",
                "mod private { pub struct Hidden; impl Hidden { pub fn op(&self) {} } } \
                 pub type Public = axum::extract::State<private::Hidden>;",
            )],
            "cli",
            &["State"],
            &["op"],
            &[],
        ),
        (
            "bare user-wrapper alias target's impl method is recorded",
            &[(
                "src/cli/handlers.rs",
                "mod private { pub struct Hidden; impl Hidden { pub fn op(&self) {} } } \
                 pub type Public = State<private::Hidden>;",
            )],
            "cli",
            &["State"],
            &["op"],
            &[],
        ),
];

#[test]
fn pub_fn_layer_membership_classification() {
    // One table over the visibility / module-tree / wrapper rules that decide
    // whether a pub fn enters a layer's call-parity surface. Each row asserts
    // the `present` names appear and the `absent` names don't, for `layer`.
    for (label, files, layer, wrappers, present, absent) in PUB_FN_CASES {
        let names = layer_names_w(files, layer, wrappers);
        for n in *present {
            assert!(
                names.contains(*n),
                "case {label}: {n} must be present in {layer}, got {names:?}"
            );
        }
        for n in *absent {
            assert!(
                !names.contains(*n),
                "case {label}: {n} must be absent from {layer}, got {names:?}"
            );
        }
    }
}
