//! Stale `// qual:api` / `// qual:test_helper` detection.
//!
//! Unlike `qual:allow`, these two markers had no feedback loop: once written
//! they silence their checks forever, unverified. That makes a marker
//! ambiguous — it can mean "this is a real API entry point" or "this is dead
//! code nobody noticed" — and genuinely dead code hides behind the second
//! reading indefinitely.
//!
//! Both markers exist to excuse a function that *production never calls*:
//! `qual:api` for an entry point called from outside the crate, and
//! `qual:test_helper` for a helper only integration tests use. So the rule is
//! simply: **once production calls the function, the excuse is spent** and the
//! marker silences nothing.
//!
//! It deliberately does not also require the function to be tested. The two
//! markers suppress DRY-002 (dead code) *and* TQ-003 (untested), but TQ-003
//! only fires for functions that already have production callers
//! (`tq::untested`) — so for a genuine outside-the-crate entry point the
//! TQ-003 exclusion never applies anyway. Requiring "and tested" would only
//! let a spent marker keep hiding a real TQ-003 finding. Removing a spent
//! marker can therefore surface an `untested` finding; that is the honest
//! signal, and the reported message says so.
//!
//! When a caller is invisible to the call graph (dynamic dispatch, macros),
//! the function reads as having no production callers and the marker is left
//! alone — the safe direction: under-report, never a false "delete me".
//!
//! Note: this rides along with the TQ pass because that is where the marked
//! declarations and the production call set already exist; with
//! `[test_quality] enabled = false` the check does not run.

use std::collections::{HashMap, HashSet};

use crate::adapters::shared::declared_function::DeclaredFunction;
use crate::adapters::shared::reachability::ExternalReach;
use crate::domain::findings::{MarkerKind, OrphanKind, OrphanSuppression};

/// Report `qual:api` / `qual:test_helper` markers that silence nothing.
/// Integration: per-declaration classification + marker-line lookup.
pub(crate) fn detect_stale_marker_orphans(
    declared: &[DeclaredFunction],
    prod_calls: &HashSet<String>,
    api_lines: &HashMap<String, HashSet<usize>>,
    test_helper_lines: &HashMap<String, HashSet<usize>>,
    reach: &ExternalReach,
) -> Vec<OrphanSuppression> {
    let mut out: Vec<OrphanSuppression> = declared
        .iter()
        .filter_map(|d| classify(d, prod_calls, reach).map(|verdict| (d, verdict)))
        .filter_map(|(d, verdict)| {
            let (marker, lines) = if d.is_api {
                (MarkerKind::Api, api_lines)
            } else {
                (MarkerKind::TestHelper, test_helper_lines)
            };
            marker_line(lines, &d.file, d.line).map(|line| orphan(d, line, marker, verdict))
        })
        .collect();
    out.extend(unattached_orphans(
        declared,
        api_lines,
        MarkerKind::Api,
        "qual:api",
    ));
    out.extend(unattached_orphans(
        declared,
        test_helper_lines,
        MarkerKind::TestHelper,
        "qual:test_helper",
    ));
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

/// Markers that no declared function claims. Both markers only ever affect
/// function-level checks (DRY-002, TQ-003), so one sitting on a type, a
/// constant or a `pub use` re-export provably changes nothing — and would
/// otherwise stay an unverified silencer forever.
///
/// Attachment mirrors `mark_api_declarations` exactly (same annotation
/// window): a marker counts as claimed precisely when that function-marking
/// pass would have picked it up, so this can never call a working marker
/// unattached. Operation: set difference over the claimed lines.
fn unattached_orphans(
    declared: &[DeclaredFunction],
    lines: &HashMap<String, HashSet<usize>>,
    marker: MarkerKind,
    what: &str,
) -> Vec<OrphanSuppression> {
    let claimed: HashSet<(&str, usize)> = declared
        .iter()
        .filter_map(|d| marker_line(lines, &d.file, d.line).map(|l| (d.file.as_str(), l)))
        .collect();
    lines
        .iter()
        .flat_map(|(file, file_lines)| file_lines.iter().map(move |l| (file, *l)))
        .filter(|(file, line)| !claimed.contains(&(file.as_str(), *line)))
        .map(|(file, line)| OrphanSuppression {
            marker,
            file: file.clone(),
            line,
            dimensions: Vec::new(),
            target: None,
            reason: Some(reason_for(Verdict::NotAttached, what, "")),
            kind: OrphanKind::Stale,
        })
        .collect()
}

/// Why a marker is being reported — each variant maps to a different remedy,
/// so the message can tell the author exactly what to do.
#[derive(Clone, Copy)]
enum Verdict {
    /// The marker reaches no function at all — it sits on a type, a constant,
    /// a `pub use` re-export … Both markers only affect function-level checks,
    /// so there it changes nothing.
    NotAttached,
    /// Attached, but that function is excluded from both checks anyway
    /// (`main`, a test fn, a trait-impl method, `#[allow(dead_code)]`).
    NoEffectOnExemptFn,
    /// `qual:api` on an item no outside consumer can name: the marker's
    /// premise is false. Production already calls it, so just drop the marker.
    NeverAppliedButCalled,
    /// `qual:api` on an unreachable item that nothing calls either — the
    /// marker is hiding dead code.
    NeverAppliedAndUncalled,
    /// The marker was right once, but production calls the function now.
    Spent,
}

/// Decide whether a marked declaration must be reported, and why.
/// `qual:test_helper` is judged only by callers: being unreachable from
/// outside the crate is its normal, intended state.
/// Integration: caller lookup + reachability question.
fn classify(
    d: &DeclaredFunction,
    prod_calls: &HashSet<String>,
    reach: &ExternalReach,
) -> Option<Verdict> {
    if !d.is_api && !d.is_test_helper {
        return None;
    }
    // Excluded from DRY-002 and TQ-003 whatever the marker says, so the marker
    // cannot be doing anything here (mirrors `should_exclude_uncalled` minus
    // the marker flags themselves, and `tq::untested`).
    if d.is_main || d.is_test || d.is_trait_impl || d.has_allow_dead_code {
        return Some(Verdict::NoEffectOnExemptFn);
    }
    let called = prod_calls.contains(&d.name) || prod_calls.contains(&d.qualified_name);
    if d.is_test_helper {
        return called.then_some(Verdict::Spent);
    }
    match (reach.is_externally_reachable(&d.file, &d.name), called) {
        (false, true) => Some(Verdict::NeverAppliedButCalled),
        (false, false) => Some(Verdict::NeverAppliedAndUncalled),
        (true, true) => Some(Verdict::Spent),
        (true, false) => None,
    }
}

/// The line the marker itself sits on: the closest annotation line at or above
/// the declaration within the annotation window. Reported instead of the
/// function line so the author can jump straight to the comment to delete.
/// Operation: window scan, no own calls.
fn marker_line(
    lines: &HashMap<String, HashSet<usize>>,
    file: &str,
    fn_line: usize,
) -> Option<usize> {
    let file_lines = lines.get(file)?;
    (0..=crate::findings::ANNOTATION_WINDOW)
        .filter(|off| fn_line >= *off)
        .map(|off| fn_line - off)
        .find(|candidate| file_lines.contains(candidate))
}

/// Build the orphan finding for one stale marker.
/// Operation: struct construction, no own calls.
fn orphan(
    d: &DeclaredFunction,
    line: usize,
    marker: MarkerKind,
    verdict: Verdict,
) -> OrphanSuppression {
    let what = match marker {
        MarkerKind::TestHelper => "qual:test_helper",
        _ => "qual:api",
    };
    OrphanSuppression {
        marker,
        file: d.file.clone(),
        line,
        dimensions: Vec::new(),
        target: None,
        reason: Some(reason_for(verdict, what, &d.qualified_name)),
        kind: OrphanKind::Stale,
    }
}

/// The remedy text for one verdict — each says what the author must do next,
/// and what will happen once they do it.
/// Operation: verdict → message, no own calls.
fn reason_for(verdict: Verdict, what: &str, name: &str) -> String {
    match verdict {
        Verdict::NotAttached => format!(
            "{what} is not attached to any function — it only affects the \
             function-level checks (dead code, untested), so on a type, a \
             constant or a `pub use` re-export it does nothing: remove it"
        ),
        Verdict::NoEffectOnExemptFn => format!(
            "{what} changes nothing for {name}: that function is already exempt \
             from the dead-code and untested checks (it is `main`, a test, a \
             trait-impl method, or carries #[allow(dead_code)]) — remove the marker"
        ),
        Verdict::NeverAppliedButCalled => format!(
            "{what} never applied here: {name} cannot be called from outside the crate \
             (it is not `pub`, or a module on its path is private), and production \
             already calls it — remove the marker"
        ),
        Verdict::NeverAppliedAndUncalled => format!(
            "{what} never applied here: {name} cannot be called from outside the crate \
             (it is not `pub`, or a module on its path is private), so there is no \
             external caller to excuse — call it from production or delete it \
             (removing the marker will surface the dead-code finding)"
        ),
        Verdict::Spent => format!(
            "production calls {name}, so {what} excuses nothing — remove the marker \
             (if the function is untested, an untested finding will surface: that is the point)"
        ),
    }
}
