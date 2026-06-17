//! Cohesion / LCOM4 + struct-SRP-warning tests. Split into focused
//! sub-files (each ≤ the SRP file-length cap); shared imports, the
//! `make_*`/`collect_methods_for` helpers, and the case-type aliases live
//! here and reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::srp::cohesion::*;
pub(super) use crate::adapters::analyzers::srp::{MethodFieldData, StructInfo};
pub(super) use crate::config::sections::SrpConfig;
pub(super) use std::collections::{HashMap, HashSet};

mod lcom4_and_collector;
mod scoring;
mod warnings;

/// An LCOM4 scenario: `(label, methods, fields, expected_lcom4,
/// expected_clusters)`. Each method is `(name, field_accesses, call_targets)`
/// on parent `Foo`. `expected_clusters = None` skips the cluster-count check.
/// A tuple (not a struct) keeps the data table clear of BP-009.
pub(super) type Lcom4Case = (
    &'static str,
    &'static [(
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
    )],
    &'static [&'static str],
    usize,
    Option<usize>,
);

/// lcom4 constructor-connection case: `(label, field-accessing methods as
/// (name, fields), constructor names (return Self, connect every field),
/// struct fields, expected lcom4)`.
pub(super) type Lcom4CtorCase = (
    &'static str,
    &'static [(&'static str, &'static [&'static str])],
    &'static [&'static str],
    &'static [&'static str],
    usize,
);

pub(super) fn make_method(
    name: &str,
    parent: &str,
    fields: &[&str],
    calls: &[&str],
) -> MethodFieldData {
    MethodFieldData {
        method_name: name.to_string(),
        parent_type: parent.to_string(),
        // Bare parent as the pooling key: these helpers pool by the same bare
        // name used for `make_struct`, so the unit tests stay file/module-free.
        owner_key: parent.to_string(),
        field_accesses: fields.iter().map(|s| s.to_string()).collect(),
        call_targets: calls.iter().map(|s| s.to_string()).collect(),
        self_method_calls: HashSet::new(),
        is_constructor: false,
    }
}

pub(super) fn make_method_with_self_calls(
    name: &str,
    parent: &str,
    fields: &[&str],
    self_calls: &[&str],
) -> MethodFieldData {
    MethodFieldData {
        method_name: name.to_string(),
        parent_type: parent.to_string(),
        owner_key: parent.to_string(),
        field_accesses: fields.iter().map(|s| s.to_string()).collect(),
        call_targets: HashSet::new(),
        self_method_calls: self_calls.iter().map(|s| s.to_string()).collect(),
        is_constructor: false,
    }
}

pub(super) fn make_constructor(name: &str, parent: &str, calls: &[&str]) -> MethodFieldData {
    MethodFieldData {
        method_name: name.to_string(),
        parent_type: parent.to_string(),
        owner_key: parent.to_string(),
        field_accesses: HashSet::new(),
        call_targets: calls.iter().map(|s| s.to_string()).collect(),
        self_method_calls: HashSet::new(),
        is_constructor: true,
    }
}

pub(super) fn make_struct(name: &str, fields: &[&str]) -> StructInfo {
    StructInfo {
        owner_key: name.to_string(),
        name: name.to_string(),
        file: "test.rs".to_string(),
        line: 1,
        fields: fields.iter().map(|s| s.to_string()).collect(),
    }
}

/// Run the full SRP method collection path and return method data for a
/// single struct, so assertions can see exactly what `MethodBodyVisitor`
/// produces from real source. Bug 2 root cause was that `MethodBodyVisitor`
/// didn't descend into macro token streams, so `self.validate()` inside
/// `debug_assert!(...)` was invisible to LCOM4.
pub(super) fn collect_methods_for(code: &str) -> Vec<MethodFieldData> {
    let syntax = syn::parse_file(code).expect("parse test fixture");
    let parsed = vec![("test.rs".to_string(), code.to_string(), syntax)];
    let mut result = Vec::new();
    let mut bridges = Vec::new();
    let mut collector = crate::adapters::analyzers::srp::ImplMethodCollector {
        file: String::new(),
        module_stack: Vec::new(),
        methods: &mut result,
        bridges: &mut bridges,
    };
    crate::adapters::shared::file_visitor::visit_all_files(&parsed, &mut collector);
    result
}
