//! Stdlib-combinator return-type table — Task 1.5.
//!
//! Encodes the subset of `Result<T,E>` / `Option<T>` / `Future<T>`
//! methods whose return type is derivable from the receiver type alone
//! — without closure-body inference. Called from
//! `infer::call::lookup_method_on_type` when the receiver is a stdlib
//! wrapper and the method isn't a user-defined impl method.
//!
//! Methods whose return type depends on a closure's output (`map`,
//! `and_then`, `then`, `filter_map`) intentionally yield `None` rather
//! than `Opaque` — returning `None` lets the caller fall through cleanly
//! to the `<method>:name` fallback without a fake edge.
//!
//! See `docs/rustqual-design-receiver-type-inference.md` §4 for the
//! normative table.

use super::canonical::CanonicalType;

// qual:api
/// Resolve a method call on a stdlib-wrapper receiver. Returns `Some(T)`
/// when the table has an entry, `None` when the method isn't covered or
/// depends on closure-body inference.
/// Integration: dispatches on wrapper kind.
pub fn combinator_return(receiver: &CanonicalType, method: &str) -> Option<CanonicalType> {
    match receiver {
        CanonicalType::Result(inner) => result_combinator(method, inner),
        CanonicalType::Option(inner) => option_combinator(method, inner),
        CanonicalType::Future(inner) => future_combinator(method, inner),
        _ => None,
    }
}

/// `Result<T, E>` methods. `T` is the Ok-side we track; `E` is erased.
/// Operation: lookup table.
fn result_combinator(method: &str, inner: &CanonicalType) -> Option<CanonicalType> {
    match method {
        // Unwrappers → T
        "unwrap" | "expect" | "unwrap_or" | "unwrap_or_else" | "unwrap_or_default" | "into_ok" => {
            Some(inner.clone())
        }
        // Transformations + observers that preserve Result<T, E>.
        // `inspect` / `inspect_err` hand the closure a borrow and
        // return self — the closure's body type doesn't change the
        // wrapper, so they stay resolved.
        "map_err" | "or_else" | "inspect" | "inspect_err" => {
            Some(CanonicalType::Result(Box::new(inner.clone())))
        }
        // Extract the Ok-side as Option<T>
        "ok" => Some(CanonicalType::Option(Box::new(inner.clone()))),
        // Extract the Err-side — E is opaque, so Option<Opaque>.
        "err" => Some(CanonicalType::Option(Box::new(CanonicalType::Opaque))),
        // Closure-dependent (change T via user closure) → unresolved.
        "map" | "and_then" => None,
        _ => None,
    }
}

/// `Option<T>` methods. Operation: lookup table.
fn option_combinator(method: &str, inner: &CanonicalType) -> Option<CanonicalType> {
    match method {
        // Unwrappers → T
        "unwrap" | "expect" | "unwrap_or" | "unwrap_or_else" | "unwrap_or_default" => {
            Some(inner.clone())
        }
        // Conversions to Result<T, _>
        "ok_or" | "ok_or_else" => Some(CanonicalType::Result(Box::new(inner.clone()))),
        // Preserve Option<T>. `inspect` is an observer — closure type
        // doesn't change the wrapper, so it stays resolved alongside
        // the structural preservers.
        "or" | "or_else" | "filter" | "take" | "replace" | "as_ref" | "as_mut" | "cloned"
        | "copied" | "inspect" => Some(CanonicalType::Option(Box::new(inner.clone()))),
        // Closure-dependent → unresolved.
        "map" | "and_then" | "map_or" | "map_or_else" => None,
        _ => None,
    }
}

/// `Future<Output=T>` methods. The canonical unwrap via `.await` is
/// handled in `infer::access::infer_await` (not a method call).
/// Out-of-scope methods like `.boxed()` / `.shared()` stay unresolved.
/// Operation: stub return; placeholder for future (Stage 2+) expansion.
fn future_combinator(_method: &str, _inner: &CanonicalType) -> Option<CanonicalType> {
    None
}
