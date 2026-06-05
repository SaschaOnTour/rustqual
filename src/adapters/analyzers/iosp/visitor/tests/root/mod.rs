//! `BodyVisitor` tests: construction/context, complexity tracking
//! (cognitive/cyclomatic/hotspots), magic-number + delegation detection.
//! Split into focused sub-files (each ≤ the SRP file-length cap); shared
//! imports + the `empty_scope`/`visit_code`/`parse_match_arms` helpers live
//! here and reach the sub-modules via `use super::*`.

pub(super) use crate::adapters::analyzers::iosp::visitor::*;
pub(super) use crate::adapters::shared::project_scope::ProjectScope;
pub(super) use crate::config::Config;
pub(super) use std::collections::HashMap;
pub(super) use syn::visit::Visit;

mod complexity;
mod construction;
mod magic_and_delegation;

pub(super) fn empty_scope() -> ProjectScope {
    ProjectScope::default()
}

pub(super) fn visit_code(code: &str) -> BodyVisitor<'static> {
    // Leak config and scope to satisfy lifetime requirements
    let config: &'static Config = Box::leak(Box::default());
    let scope: &'static ProjectScope = Box::leak(Box::default());
    let mut visitor = BodyVisitor::new(config, scope, Some("test_fn"), None, HashMap::new());
    let block: syn::Block = syn::parse_str(&format!("{{ {code} }}")).unwrap();
    block.stmts.iter().for_each(|stmt| visitor.visit_stmt(stmt));
    visitor
}

pub(super) fn parse_match_arms(code: &str) -> Vec<syn::Arm> {
    let expr: syn::ExprMatch = syn::parse_str(code).unwrap();
    expr.arms
}
