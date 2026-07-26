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
    /// Trait object / `impl Trait` return position: each element is
    /// one trait's canonical path (`["crate", "app", "Handler"]`).
    /// Multiple elements model multi-bound spellings — `dyn T1 + T2`
    /// and `impl T1 + T2` collect every resolvable trait so
    /// receiver-position dispatch can fan out one edge per trait via
    /// `trait_dispatch_edges`. The first trait that defines the
    /// called method wins for return-type inference. Distinct from
    /// `GenericParamBound`: this variant carries no substitutable
    /// generic param — turbofish on the call site does NOT replace
    /// it (the `impl Trait` opaque type isn't named by the param).
    TraitBound(Vec<Vec<String>>),
    /// Bare fn-generic-param ident with its trait bounds — the return
    /// type IS one of the callee's own type parameters
    /// (`fn f<Q: T>() -> Q`). Carries the same dispatch info as
    /// `TraitBound` (every consumer of TraitBound MUST also handle
    /// this variant for dispatch; both flow through
    /// `as_trait_bounds()`). The distinction matters at exactly one
    /// site: `infer_call` / `infer_method_call` let an explicit
    /// turbofish substitute the concrete type, because the caller IS
    /// naming the param — strictly more specific than the bound.
    /// `turbofish_index` is the position of this param in the
    /// call-site-substitutable generics list (method-own params for
    /// methods, fn-own params for free fns / structs); `None` means
    /// the param can't be substituted via a call-site turbofish
    /// (e.g. an impl-level `<Q>` referenced from a method return).
    GenericParamBound {
        bounds: Vec<Vec<String>>,
        turbofish_index: Option<usize>,
    },
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

    /// True iff this is `Opaque` — used by the builder to decide whether
    /// to cache-populate an entry for generic-return fns. Operation.
    pub fn is_opaque(&self) -> bool {
        matches!(self, Self::Opaque)
    }

    /// Return the `Ok`/`Some`/`Output` inner type for the three stdlib
    /// wrappers. Used by `?` / `.await` / stdlib-combinator resolution.
    /// Returns `None` for non-wrapper variants. Operation.
    pub fn happy_inner(&self) -> Option<&CanonicalType> {
        match self {
            Self::Result(inner) | Self::Option(inner) | Self::Future(inner) => Some(inner),
            _ => None,
        }
    }

    /// Trait-dispatch bounds regardless of origin — `Some(&bounds)` for
    /// both `TraitBound` (impl/dyn Trait) and `GenericParamBound` (bare
    /// generic-param ident). Both forms dispatch the same way through
    /// the trait anchor index. Returns `None` for any other variant.
    /// The single source of truth for "do these bounds support
    /// trait-dispatch?" — every consumer that used to match
    /// `CanonicalType::TraitBound` must go through this helper so the
    /// `GenericParamBound` variant doesn't silently miss dispatch.
    pub fn as_trait_bounds(&self) -> Option<&[Vec<String>]> {
        match self {
            Self::TraitBound(b) | Self::GenericParamBound { bounds: b, .. } => Some(b),
            _ => None,
        }
    }
}
