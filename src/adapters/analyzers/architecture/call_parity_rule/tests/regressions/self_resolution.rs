use super::*;

#[test]
fn self_method_call_resolves_via_impl_type() {
    // `impl Session { fn run(&self) { self.diff() } }` — `self` must
    // bind to the enclosing impl's canonical type so `self.diff()`
    // routes through `method_returns[Session::diff]` instead of
    // collapsing to `<method>:diff`.
    let fx = parse(
        r#"
        impl Session {
            pub fn run(&self) {
                self.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &sample_session_index(), "Session", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "self.diff() must route through workspace_index, got {calls:?}"
    );
}

#[test]
fn self_field_access_resolves_via_impl_type() {
    // `self.session.diff()` — Self::session field, then Session::diff.
    // Needs both the Self → Ctx binding and the field-type lookup
    // chain to fire.
    let fx = parse(
        r#"
        impl Ctx {
            pub fn run(&self) {
                self.session.diff();
            }
        }
        "#,
    );
    let self_segs = vec!["crate".to_string(), "app".to_string(), "Ctx".to_string()];
    let calls = run_impl_method(&fx, &sample_session_index(), "Ctx", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "self.session.diff() must chain through field type, got {calls:?}"
    );
}

#[test]
fn signature_param_typed_self_resolves() {
    // `fn merge(&self, other: Self)` inside `impl Session` — `other`
    // is declared as `Self`, must bind to `Session` so `other.diff()`
    // routes through `method_returns`.
    let fx = parse(
        r#"
        impl Session {
            pub fn merge(&self, other: Self) {
                other.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &sample_session_index(), "Session", "merge", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "param `other: Self` must resolve to Session, got {calls:?}"
    );
}

#[test]
fn let_annotation_self_resolves() {
    // `let other: Self = make();` inside `impl Session` — annotation
    // must substitute Self before resolving.
    let fx = parse(
        r#"
        impl Session {
            pub fn run(&self) {
                let other: Self = make();
                other.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &sample_session_index(), "Session", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "`let other: Self = …` must bind to Session, got {calls:?}"
    );
}

#[test]
fn turbofish_self_inside_impl_resolves() {
    // `let s = get::<Self>(); s.diff();` inside `impl Session`. The
    // turbofish-as-return-type fallback must substitute Self before
    // resolving the type argument so the binding pins to Session.
    let fx = parse(
        r#"
        impl Session {
            pub fn run(&self) {
                let s = get::<Self>();
                s.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &sample_session_index(), "Session", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "`get::<Self>()` turbofish must resolve to Session, got {calls:?}"
    );
}

#[test]
fn annotated_destructuring_self_resolves() {
    // `let Some(other): Option<Self> = maybe() else { return; };` —
    // the annotation goes through `bind_annotated` in the destructure
    // walker, which must substitute Self before resolving.
    let fx = parse(
        r#"
        impl Session {
            pub fn run(&self) {
                let Some(other): Option<Self> = maybe() else { return; };
                other.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &sample_session_index(), "Session", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "annotated destructuring with Self must bind to Session, got {calls:?}"
    );
}

#[test]
fn cast_as_self_resolves() {
    // `(expr as Self).diff()` inside `impl Session` — `infer_cast`
    // resolves the target type, which must substitute Self.
    let fx = parse(
        r#"
        impl Session {
            pub fn run(&self) {
                let s = (raw() as Self);
                s.diff();
            }
        }
        "#,
    );
    let self_segs = vec![
        "crate".to_string(),
        "app".to_string(),
        "session".to_string(),
        "Session".to_string(),
    ];
    let calls = run_impl_method(&fx, &sample_session_index(), "Session", "run", self_segs);
    assert!(
        calls.contains("crate::app::session::Session::diff"),
        "`as Self` cast must resolve to Session, got {calls:?}"
    );
}
