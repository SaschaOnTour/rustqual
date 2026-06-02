use super::*;

// ── Empty / trivial ──────────────────────────────────────────────

#[test]
fn test_empty_workspace_produces_empty_index() {
    let index = index_for(&[], &[]);
    assert!(index.struct_fields.is_empty());
    assert!(index.method_returns.is_empty());
    assert!(index.fn_returns.is_empty());
}

// ── struct_fields ────────────────────────────────────────────────

/// A struct-field indexing case: `(label, files, roots, expect)`.
/// `expect = Some((struct, field, type))` means that field resolves to that
/// workspace type; `None` means nothing is indexed at all (tuple structs,
/// opaque/non-workspace field types). A tuple (not a struct) so the data
/// table doesn't read as repeated struct-construction boilerplate.
type FieldCase = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static [&'static str],
    Option<(&'static str, &'static str, &'static [&'static str])>,
);

#[test]
fn struct_field_indexing() {
    let cases: [FieldCase; 4] = [
        // Field type must be a workspace-local type — stdlib `String` would
        // resolve to `Opaque` and get skipped by `record_field`.
        (
            "named field of a workspace type is indexed",
            &[(
                "src/app/session.rs",
                "pub struct Id;\npub struct Session { pub id: Id }",
            )],
            &["src/app/session.rs"],
            Some((
                "crate::app::session::Session",
                "id",
                &["crate", "app", "session", "Id"],
            )),
        ),
        (
            "Arc<T> field is stripped to T",
            &[(
                "src/app/context.rs",
                "pub struct Inner { pub v: u8 }\npub struct Ctx { pub inner: std::sync::Arc<Inner> }",
            )],
            &["src/app/context.rs"],
            Some((
                "crate::app::context::Ctx",
                "inner",
                &["crate", "app", "context", "Inner"],
            )),
        ),
        (
            "tuple struct is not indexed",
            &[("src/app/foo.rs", "pub struct Id(pub String);")],
            &["src/app/foo.rs"],
            None,
        ),
        (
            "opaque (non-workspace) field type is skipped",
            &[(
                "src/app/foo.rs",
                "pub struct Ctx { pub x: external_crate::Unknown }",
            )],
            &["src/app/foo.rs"],
            None,
        ),
    ];
    for (label, files, roots, expect) in cases {
        let index = index_for(files, roots);
        match expect {
            Some((struct_canonical, field, ty)) => assert_eq!(
                index.struct_field(struct_canonical, field),
                Some(&CanonicalType::path(ty.iter().copied())),
                "case: {label}"
            ),
            None => assert!(index.struct_fields.is_empty(), "case: {label}"),
        }
    }
}
