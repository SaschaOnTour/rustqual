//! `CanonicalType` — the currency of shallow type inference.
//!
//! Every value is either a crate-rooted concrete type
//! (`Path(["crate", "app", "Session"])`), a recognised stdlib wrapper
//! (`Result`/`Option`/`Future`/`Slice`/`Map`), a trait-bound (Stage 2),
//! or `Opaque` — "we looked and couldn't resolve further".
//!
//! `Opaque` is deliberately distinct from `None` at the inference API
//! boundary: `Some(Opaque)` means "we evaluated and know we can't pin
//! down the concrete type"; `None` means "we don't even have context
//! to try". Both fall back to `<method>:name` behaviour in the call
//! collector, but the distinction matters when we trace inference paths
//! during debugging.

/// Shallow-inferred type of a `syn::Expr` or declared type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CanonicalType {
    /// Crate-rooted concrete type, e.g. `["crate", "app", "session", "Session"]`.
    /// The form we look up in `WorkspaceTypeIndex`.
    Path(Vec<String>),
    /// `Result<Ok, _>` — only the Ok-side is tracked; error type is erased
    /// because call-parity resolution never walks through it.
    Result(Box<CanonicalType>),
    /// `Option<T>`.
    Option(Box<CanonicalType>),
    /// `Future<Output = T>`.
    Future(Box<CanonicalType>),
    /// Iterator element — produced from `Vec<T>` / `&[T]` / `[T; N]`.
    Slice(Box<CanonicalType>),
    /// `HashMap<_, V>` — only the value type is tracked.
    Map(Box<CanonicalType>),
    /// Trait object / generic bound. Reserved for Stage 2 — Stage 1 never
    /// emits this variant.
    TraitBound(Vec<String>),
    /// Locally known to be unresolvable: external crate, unannotated
    /// generic, unsupported construct. Distinct from "not yet evaluated".
    Opaque,
}

impl CanonicalType {
    /// Construct a `Path` variant from any iterator of string-ish segments.
    /// Operation: pure mapping.
    pub fn path<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Path(segments.into_iter().map(Into::into).collect())
    }

    // qual:api
    /// True iff this is `Opaque` — used by the builder to decide whether
    /// to cache-populate an entry for generic-return fns. Operation.
    pub fn is_opaque(&self) -> bool {
        matches!(self, Self::Opaque)
    }

    // qual:api
    /// Return the `Ok`/`Some`/`Output` inner type for the three stdlib
    /// wrappers. Used by `?` / `.await` / stdlib-combinator resolution.
    /// Returns `None` for non-wrapper variants. Operation.
    pub fn happy_inner(&self) -> Option<&CanonicalType> {
        match self {
            Self::Result(inner) | Self::Option(inner) | Self::Future(inner) => Some(inner),
            _ => None,
        }
    }
}
