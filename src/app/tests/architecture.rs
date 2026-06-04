//! `app::architecture` glue: window-scoped suppression marking and the
//! summary warning count. Built on hand-constructed `Finding`s so the
//! port-based analyzer doesn't need to run.
use crate::app::architecture::{count_architecture_warnings, mark_architecture_suppressions};
use crate::domain::{Dimension, Finding, Severity};
use std::collections::HashMap;

fn arch_finding(line: usize, suppressed: bool) -> Finding {
    Finding {
        file: "src/x.rs".into(),
        line,
        column: 0,
        dimension: Dimension::Architecture,
        rule_id: "architecture/layer".into(),
        message: String::new(),
        severity: Severity::Medium,
        suppressed,
    }
}

fn marker(line: usize, dim: Dimension) -> HashMap<String, Vec<crate::findings::Suppression>> {
    [(
        "src/x.rs".to_string(),
        vec![crate::findings::Suppression {
            line,
            dimensions: vec![dim],
            reason: None,
        }],
    )]
    .into()
}

#[test]
fn architecture_marker_in_window_suppresses_finding() {
    // A `qual:allow(architecture)` at line 5 covers a finding at line 6 (within
    // ANNOTATION_WINDOW). Pins `mark_architecture_suppressions` against being a
    // no-op (`-> ()`), which would leave the finding active.
    let mut findings = vec![arch_finding(6, false)];
    mark_architecture_suppressions(&mut findings, &marker(5, Dimension::Architecture));
    assert!(
        findings[0].suppressed,
        "an in-window architecture marker must suppress the finding"
    );
}

#[test]
fn architecture_marker_must_cover_the_architecture_dimension() {
    // A marker in the window but covering a *different* dimension must not
    // suppress an architecture finding. Pins the `covers(Architecture) &&
    // is_within_window` filter against `&&`→`||` (which would suppress on
    // window proximity alone).
    let mut findings = vec![arch_finding(6, false)];
    mark_architecture_suppressions(&mut findings, &marker(5, Dimension::Complexity));
    assert!(
        !findings[0].suppressed,
        "an in-window marker for another dimension must not suppress architecture"
    );
}

#[test]
fn count_architecture_warnings_excludes_suppressed() {
    // Two active + one suppressed → 2. The asymmetric counts pin the
    // `!suppressed` filter: deleting the `!` would count the one suppressed
    // finding instead (1 ≠ 2).
    let findings = vec![
        arch_finding(1, false),
        arch_finding(2, false),
        arch_finding(3, true),
    ];
    assert_eq!(count_architecture_warnings(&findings), 2);
}
