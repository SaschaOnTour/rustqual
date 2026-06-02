mod derive;
mod function_call;
mod glob_import;
mod item_kind;
mod macro_call;
mod method_call;
mod path_prefix;

use crate::adapters::analyzers::architecture::{MatchLocation, ViolationKind};

/// Assert that `hits` holds exactly one match whose method/macro name is
/// `expected`. Shared by the method-call and macro-call matcher tests, whose
/// "one hit, check the name" assertion is otherwise identical.
pub(super) fn assert_one_named(hits: &[MatchLocation], expected: &str) {
    assert_eq!(hits.len(), 1, "expected one hit: {hits:?}");
    let name = match &hits[0].kind {
        ViolationKind::MethodCall { name, .. } | ViolationKind::MacroCall { name } => name,
        other => panic!("unexpected kind: {other:?}"),
    };
    assert_eq!(name, expected);
}
