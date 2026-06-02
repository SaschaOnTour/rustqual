//! Tests for app::metrics — parameter-count SRP warnings, SDP/DRY/inverse
//! suppression marking. Split into focused sub-files (each ≤ the SRP
//! file-length cap); shared imports + make_func/make_srp/dry_suppression_at/
//! dry_group_fixtures helpers live here and reach the sub-modules via
//! `use super::*`.

pub(super) use crate::adapters::analyzers::dry::boilerplate::BoilerplateFind;
pub(super) use crate::adapters::analyzers::dry::fragments::{FragmentEntry, FragmentGroup};
pub(super) use crate::adapters::analyzers::dry::functions::{
    DuplicateEntry, DuplicateGroup, DuplicateKind,
};
pub(super) use crate::adapters::analyzers::dry::match_patterns::{
    RepeatedMatchEntry, RepeatedMatchGroup,
};
pub(super) use crate::adapters::analyzers::iosp::{
    compute_severity, Classification, FunctionAnalysis,
};
pub(super) use crate::app::dry_suppressions::{mark_dry_suppressions, mark_inverse_suppressions};
pub(super) use crate::app::metrics::*;
pub(super) use crate::config::sections::SrpConfig;
pub(super) use crate::findings::Suppression;
pub(super) use crate::report::Summary;

mod param_warnings;
mod suppression;

pub(super) fn make_func(name: &str, param_count: usize, trait_impl: bool) -> FunctionAnalysis {
    let severity = compute_severity(&Classification::Operation);
    FunctionAnalysis {
        name: name.to_string(),
        file: "test.rs".to_string(),
        line: 1,
        classification: Classification::Operation,
        parent_type: None,
        suppressed: false,
        complexity: None,
        qualified_name: name.to_string(),
        severity,
        cognitive_warning: false,
        cyclomatic_warning: false,
        nesting_depth_warning: false,
        function_length_warning: false,
        unsafe_warning: false,
        error_handling_warning: false,
        complexity_suppressed: false,
        own_calls: vec![],
        parameter_count: param_count,
        is_trait_impl: trait_impl,
        is_test: false,
        effort_score: None,
    }
}

pub(super) fn make_srp() -> crate::adapters::analyzers::srp::SrpAnalysis {
    crate::adapters::analyzers::srp::SrpAnalysis {
        struct_warnings: vec![],
        module_warnings: vec![],
        param_warnings: vec![],
    }
}

/// A single `qual:allow(dry)` suppression at `line` in `test.rs`.
pub(super) fn dry_suppression_at(
    line: usize,
) -> std::collections::HashMap<String, Vec<Suppression>> {
    [(
        "test.rs".to_string(),
        vec![Suppression {
            line,
            dimensions: vec![crate::findings::Dimension::Dry],
            reason: None,
        }],
    )]
    .into()
}

pub(super) type DryFixtures = (
    DuplicateGroup,
    RepeatedMatchGroup,
    FragmentGroup,
    BoilerplateFind,
);

/// One fixture per DRY group kind, each with an entry on line 5.
pub(super) fn dry_group_fixtures() -> DryFixtures {
    (
        DuplicateGroup {
            entries: vec![DuplicateEntry {
                name: "as_str".to_string(),
                qualified_name: "Foo::as_str".to_string(),
                file: "test.rs".to_string(),
                line: 5,
            }],
            kind: DuplicateKind::NearDuplicate { similarity: 0.91 },
            suppressed: false,
        },
        RepeatedMatchGroup {
            enum_name: "MyEnum".to_string(),
            entries: vec![RepeatedMatchEntry {
                file: "test.rs".to_string(),
                line: 5,
                function_name: "handle_a".to_string(),
                arm_count: 5,
            }],
            suppressed: false,
        },
        FragmentGroup {
            entries: vec![FragmentEntry {
                function_name: "foo".to_string(),
                qualified_name: "foo".to_string(),
                file: "test.rs".to_string(),
                start_line: 5,
                end_line: 10,
            }],
            statement_count: 3,
            suppressed: false,
        },
        BoilerplateFind {
            pattern_id: "BP-003".to_string(),
            file: "test.rs".to_string(),
            line: 5,
            struct_name: Some("MyStruct".to_string()),
            description: "3 trivial getters".to_string(),
            suggestion: "Consider derive macro".to_string(),
            suppressed: false,
        },
    )
}
