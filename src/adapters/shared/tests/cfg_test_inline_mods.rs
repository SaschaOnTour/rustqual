//! cfg-test classification through inline `mod {}` blocks.
//!
//! A `mod name;` nested in an inline block is a real declaration — it just
//! lives in the block's directory rather than the file's, so a scan of a
//! file's top-level items alone never sees it and its file is analysed as
//! production code.

use super::cfg_test_files::{assert_classification, ClassifyCase};

#[test]
fn cfg_test_status_reaches_declarations_inside_inline_modules() {
    let cases: &[ClassifyCase] = &[
        // A `mod` declaration nested in an inline `mod { … }` block is a real
        // declaration. Scanning only a file's top-level items misses it, and
        // the test-only file it names is then analysed as production.
        (
            "a #[cfg(test)] mod inside an inline module is classified",
            &[
                (
                    "src/parent.rs",
                    "mod helpers {\n    #[cfg(test)]\n    mod tests;\n}",
                ),
                ("src/parent/helpers/tests.rs", "pub fn helper() {}"),
            ],
            &["src/parent/helpers/tests.rs"],
            &[],
        ),
        // `#[cfg(test)]` on an inline block covers everything it declares —
        // the block's own children are test-only without carrying the
        // attribute themselves.
        (
            "an out-of-line mod inside a #[cfg(test)] inline block is classified",
            &[
                (
                    "src/parent.rs",
                    "#[cfg(test)]\nmod tests {\n    mod golden;\n}",
                ),
                ("src/parent/tests/golden.rs", "pub fn helper() {}"),
            ],
            &["src/parent/tests/golden.rs"],
            &[],
        ),
        // The counterpart: descending into inline blocks must not classify
        // ordinary production modules found there.
        (
            "a plain mod inside a plain inline block stays production",
            &[
                ("src/prod.rs", "mod helpers {\n    mod util;\n}"),
                ("src/prod/helpers/util.rs", "pub fn u() {}"),
            ],
            &[],
            &["src/prod/helpers/util.rs"],
        ),
    ];
    assert_classification(cases);
}
