//! Unit tests for `syn::Type` → `CanonicalType` conversion.
//!
//! The `resolve` module is `pub(super)` — these tests live in the same
//! crate and reach it via the in-crate module path.

use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::canonical::CanonicalType;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::resolve::{
    resolve_type, ResolveContext,
};
use std::collections::{HashMap, HashSet};

fn parse_type(src: &str) -> syn::Type {
    syn::parse_str(src).expect("parse type")
}

fn ctx<'a>(
    alias_map: &'a HashMap<String, Vec<String>>,
    local_symbols: &'a HashSet<String>,
    crate_root_modules: &'a HashSet<String>,
    importing_file: &'a str,
) -> ResolveContext<'a> {
    ResolveContext {
        alias_map,
        local_symbols,
        crate_root_modules,
        importing_file,
        type_aliases: None,
        transparent_wrappers: None,
    }
}

#[test]
fn test_bare_path_resolves_via_local_symbols() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Session");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/app/session.rs"));
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_reference_type_strips_and_recurses() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    let ty = parse_type("&Session");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/app/session.rs"));
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_result_wraps_inner() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Result<Session, Error>");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/app/session.rs"));
    match resolved {
        CanonicalType::Result(inner) => {
            assert_eq!(
                *inner,
                CanonicalType::path(["crate", "app", "session", "Session"])
            );
        }
        other => panic!("expected Result(_), got {:?}", other),
    }
}

#[test]
fn test_option_wraps_inner() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("T".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Option<T>");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/foo.rs"));
    assert!(matches!(resolved, CanonicalType::Option(_)));
}

#[test]
fn test_arc_is_stripped() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Arc<Session>");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/app/session.rs"));
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_nested_wrappers_strip_to_inner() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Session".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Arc<RwLock<Session>>");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/app/session.rs"));
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_vec_becomes_slice() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Handler".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Vec<Handler>");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/foo.rs"));
    assert!(matches!(resolved, CanonicalType::Slice(_)));
}

#[test]
fn test_hashmap_keeps_value_type() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Handler".to_string());
    let roots = HashSet::new();
    let ty = parse_type("HashMap<String, Handler>");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/foo.rs"));
    match resolved {
        CanonicalType::Map(inner) => {
            assert_eq!(*inner, CanonicalType::path(["crate", "foo", "Handler"]));
        }
        other => panic!("expected Map(_), got {:?}", other),
    }
}

#[test]
fn test_array_becomes_slice() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("T".to_string());
    let roots = HashSet::new();
    let ty = parse_type("[T; 4]");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/foo.rs"));
    assert!(matches!(resolved, CanonicalType::Slice(_)));
}

#[test]
fn test_slice_type_becomes_slice() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("T".to_string());
    let roots = HashSet::new();
    let ty = parse_type("&[T]");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/foo.rs"));
    assert!(matches!(resolved, CanonicalType::Slice(_)));
}

#[test]
fn test_trait_object_is_opaque() {
    let alias_map = HashMap::new();
    let local = HashSet::new();
    let roots = HashSet::new();
    let ty = parse_type("Box<dyn Handler>");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/foo.rs"));
    // Box<dyn T> → strip Box → dyn T → Opaque
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_impl_trait_is_opaque() {
    let alias_map = HashMap::new();
    let local = HashSet::new();
    let roots = HashSet::new();
    let ty = parse_type("impl Iterator<Item = u8>");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/foo.rs"));
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_unknown_external_path_is_opaque() {
    let alias_map = HashMap::new();
    let local = HashSet::new();
    let roots = HashSet::new();
    let ty = parse_type("external_crate::UnknownType");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/foo.rs"));
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_aliased_path_resolves_via_alias_map() {
    let mut alias_map = HashMap::new();
    alias_map.insert(
        "Session".to_string(),
        vec![
            "crate".to_string(),
            "app".to_string(),
            "session".to_string(),
            "Session".to_string(),
        ],
    );
    let local = HashSet::new();
    let roots = HashSet::new();
    let ty = parse_type("Session");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/cli/handlers.rs"));
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_future_wraps_output() {
    let alias_map = HashMap::new();
    let mut local = HashSet::new();
    local.insert("Response".to_string());
    let roots = HashSet::new();
    let ty = parse_type("Future<Response>");
    let resolved = resolve_type(&ty, &ctx(&alias_map, &local, &roots, "src/foo.rs"));
    assert!(matches!(resolved, CanonicalType::Future(_)));
}
