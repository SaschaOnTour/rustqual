use super::*;

#[test]
fn test_bare_path_resolves_via_local_symbols() {
    let resolved = resolve_in("Session", "src/app/session.rs", &["Session"], &[]);
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_reference_type_strips_and_recurses() {
    let resolved = resolve_in("&Session", "src/app/session.rs", &["Session"], &[]);
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_result_wraps_inner() {
    let resolved = resolve_in(
        "Result<Session, Error>",
        "src/app/session.rs",
        &["Session"],
        &[],
    );
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
    let resolved = resolve_in("Option<T>", "src/foo.rs", &["T"], &[]);
    assert!(matches!(resolved, CanonicalType::Option(_)));
}

#[test]
fn test_arc_is_stripped() {
    let resolved = resolve_in("Arc<Session>", "src/app/session.rs", &["Session"], &[]);
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_nested_smart_pointers_strip_to_inner() {
    // Only smart-pointer wrappers (Arc/Box/Rc/Cow) are Deref-transparent, so
    // nesting them still reaches the inner type.
    let resolved = resolve_in("Arc<Box<Session>>", "src/app/session.rs", &["Session"], &[]);
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_rwlock_is_not_peeled() {
    // `RwLock::read()` returns a guard, not the inner value — peeling it
    // would synthesize bogus `Session::read` edges. Stays `Opaque`.
    let resolved = resolve_in(
        "Arc<RwLock<Session>>",
        "src/app/session.rs",
        &["Session"],
        &[],
    );
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_vec_becomes_slice() {
    let resolved = resolve_in("Vec<Handler>", "src/foo.rs", &["Handler"], &[]);
    assert!(matches!(resolved, CanonicalType::Slice(_)));
}

#[test]
fn test_hashmap_keeps_value_type() {
    let resolved = resolve_in("HashMap<String, Handler>", "src/foo.rs", &["Handler"], &[]);
    match resolved {
        CanonicalType::Map(inner) => {
            assert_eq!(*inner, CanonicalType::path(["crate", "foo", "Handler"]));
        }
        other => panic!("expected Map(_), got {:?}", other),
    }
}

#[test]
fn test_array_becomes_slice() {
    let resolved = resolve_in("[T; 4]", "src/foo.rs", &["T"], &[]);
    assert!(matches!(resolved, CanonicalType::Slice(_)));
}

#[test]
fn test_slice_type_becomes_slice() {
    let resolved = resolve_in("&[T]", "src/foo.rs", &["T"], &[]);
    assert!(matches!(resolved, CanonicalType::Slice(_)));
}

#[test]
fn test_trait_object_unresolved_is_opaque() {
    // Box<dyn T> → strip Box → dyn T — when T isn't resolvable (not in
    // local symbols / alias map / crate roots), stays Opaque.
    let resolved = resolve_in("Box<dyn Handler>", "src/foo.rs", &[], &[]);
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_trait_object_resolves_via_local_symbols() {
    let resolved = resolve_in("Box<dyn Handler>", "src/app/mod.rs", &["Handler"], &[]);
    assert_eq!(
        resolved,
        CanonicalType::TraitBound(vec![vec![
            "crate".to_string(),
            "app".to_string(),
            "Handler".to_string(),
        ]])
    );
}

#[test]
fn test_impl_trait_unresolved_is_opaque() {
    // `Iterator` isn't in local symbols / alias map — stays Opaque.
    let resolved = resolve_in("impl Iterator<Item = u8>", "src/foo.rs", &[], &[]);
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_impl_trait_resolves_to_trait_bound() {
    // `impl Handler` return-type resolves to `TraitBound(Handler)` so
    // trait-dispatch over-approximation can fire on the method call.
    let resolved = resolve_in("impl Handler + Send", "src/app/mod.rs", &["Handler"], &[]);
    assert_eq!(
        resolved,
        CanonicalType::TraitBound(vec![vec![
            "crate".to_string(),
            "app".to_string(),
            "Handler".to_string(),
        ]])
    );
}

#[test]
fn test_unknown_external_path_is_opaque() {
    let resolved = resolve_in("external_crate::UnknownType", "src/foo.rs", &[], &[]);
    assert_eq!(resolved, CanonicalType::Opaque);
}

#[test]
fn test_aliased_path_resolves_via_alias_map() {
    let resolved = resolve_aliased(
        "Session",
        "src/cli/handlers.rs",
        &[("Session", &["crate", "app", "session", "Session"])],
        &[],
    );
    assert_eq!(
        resolved,
        CanonicalType::path(["crate", "app", "session", "Session"])
    );
}

#[test]
fn test_future_wraps_output() {
    let resolved = resolve_in("Future<Response>", "src/foo.rs", &["Response"], &[]);
    assert!(matches!(resolved, CanonicalType::Future(_)));
}
