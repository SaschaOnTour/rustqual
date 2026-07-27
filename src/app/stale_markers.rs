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
use crate::adapters::shared::declared_type::DeclaredType;
use crate::adapters::shared::reachability::ExternalReach;
use crate::domain::findings::{MarkerKind, OrphanKind, OrphanSuppression};

/// Everything the check needs about the analysed code. Bundled because the
/// two declaration kinds each bring their own "is it used" evidence.
pub(crate) struct MarkerContext<'a> {
    pub declared_fns: &'a [DeclaredFunction],
    pub declared_types: &'a [DeclaredType],
    /// Names production calls — the evidence for a function.
    pub prod_calls: &'a HashSet<String>,
    /// Names production refers to — the evidence for a type or constant.
    pub prod_refs: &'a HashSet<String>,
    pub api_lines: &'a HashMap<String, HashSet<usize>>,
    pub test_helper_lines: &'a HashMap<String, HashSet<usize>>,
    pub reach: &'a ExternalReach,
}

/// One marked declaration, whatever kind. The check asks a function and a type
/// the same three questions — is it exempt anyway, can it be named from
/// outside, does production use it — so they are answered against one shape.
struct Marked<'a> {
    file: &'a str,
    line: usize,
    /// The name external reachability is keyed by.
    name: &'a str,
    /// The name shown in the message (qualified, for a method).
    display: &'a str,
    is_api: bool,
    is_test_helper: bool,
    /// Set when the declaration is excluded from its checks whatever the
    /// marker says — the text says which exemption applies.
    exempt: Option<&'static str>,
    /// Whether production uses it; `None` when the name cannot be attributed.
    used: Option<bool>,
    /// The words that differ by declaration kind.
    kind: Kind,
}

/// What a message has to call things, which differs between a function and a
/// type: how production uses it, and which check reports it when the marker
/// goes. Getting the second wrong sends the author looking for a `DEAD_CODE`
/// finding on a struct.
#[derive(Clone, Copy)]
struct Kind {
    /// "calls" a function, "refers to" a type.
    verb: &'static str,
    /// "dead-code" for DRY-002, "dead-type" for DRY-006.
    finding: &'static str,
}

/// Operation: constant, no own calls.
const FUNCTION: Kind = Kind {
    verb: "calls",
    finding: "dead-code",
};

/// Operation: constant, no own calls.
const TYPE: Kind = Kind {
    verb: "refers to",
    finding: "dead-type",
};

/// Report `qual:api` / `qual:test_helper` markers that silence nothing.
/// Integration: marked-declaration collection + per-marker passes.
pub(crate) fn detect_stale_marker_orphans(ctx: &MarkerContext<'_>) -> Vec<OrphanSuppression> {
    let marked = collect_marked(ctx);
    // A declaration can carry BOTH markers; each is judged on its own, or the
    // second would sit unverified forever.
    let mut out = attached_orphans(&marked, ctx, MarkerKind::Api);
    out.extend(attached_orphans(&marked, ctx, MarkerKind::TestHelper));
    out.extend(unattached_orphans(&marked, ctx.api_lines, MarkerKind::Api));
    out.extend(unattached_orphans(
        &marked,
        ctx.test_helper_lines,
        MarkerKind::TestHelper,
    ));
    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
}

/// Both declaration kinds reduced to the shape the verdicts are decided on.
/// Integration: two conversions concatenated.
fn collect_marked<'a>(ctx: &'a MarkerContext<'a>) -> Vec<Marked<'a>> {
    let ambiguous = ambiguous_names(ctx);
    let mut out = marked_fns(ctx, &ambiguous);
    out.extend(marked_types(ctx, &ambiguous));
    out
}

/// Operation: per-function projection, own calls hidden in the closure.
fn marked_fns<'a>(ctx: &'a MarkerContext<'a>, ambiguous: &HashSet<&str>) -> Vec<Marked<'a>> {
    ctx.declared_fns
        .iter()
        .map(|d| Marked {
            file: &d.file,
            line: d.line,
            name: &d.name,
            display: &d.qualified_name,
            is_api: d.is_api,
            is_test_helper: d.is_test_helper,
            // Mirrors `should_exclude_uncalled` minus the marker flags, and
            // `tq::untested`.
            exempt: (d.is_main || d.is_test || d.is_trait_impl || d.dead_code_exempt).then_some(
                "it is `main`, a test, a trait-impl method, or exempt from dead-code \
                 analysis (an #[allow(dead_code)] in force, or an export such as \
                 #[no_mangle])",
            ),
            used: attributed_use(&d.qualified_name, &d.name, ctx.prod_calls, ambiguous),
            kind: FUNCTION,
        })
        .collect()
}

/// Operation: per-type projection, own calls hidden in the closure.
fn marked_types<'a>(ctx: &'a MarkerContext<'a>, ambiguous: &HashSet<&str>) -> Vec<Marked<'a>> {
    ctx.declared_types
        .iter()
        .map(|d| Marked {
            file: &d.file,
            line: d.line,
            name: &d.name,
            display: &d.name,
            is_api: d.is_api,
            is_test_helper: d.is_test_helper,
            // Mirrors `dead_types::excluded` minus the marker flags.
            exempt: (d.is_test || d.dead_code_exempt || d.name.starts_with('_')).then_some(
                "it is test-only, exempt from dead-code analysis (an \
                 #[allow(dead_code)] in force, or an export such as #[no_mangle]), \
                 or its name starts with `_`",
            ),
            used: attributed_use(&d.name, &d.name, ctx.prod_refs, ambiguous),
            kind: TYPE,
        })
        .collect()
}

/// Markers a declaration claims, with the verdict for each.
/// Integration: nearest-owner resolution + per-owner classification.
fn attached_orphans(
    marked: &[Marked<'_>],
    ctx: &MarkerContext<'_>,
    marker: MarkerKind,
) -> Vec<OrphanSuppression> {
    let lines = lines_for(ctx, marker);
    nearest_owners(marked, lines, marker)
        .into_iter()
        .filter_map(|(line, m)| {
            classify(m, ctx.reach, marker).map(|verdict| orphan(m, line, marker, verdict))
        })
        .collect()
}

/// One owner per marker line: the declaration closest below it.
///
/// The annotation window is a flat look-back, so a marker on a one-line
/// declaration is also within reach of whatever follows it. Judging every
/// declaration in reach would report the *further* one's verdict against a
/// marker that is not its own — and since the two verdicts differ, that means
/// telling the author to delete a marker that is doing its job. The marking
/// passes are deliberately left as they are: marking one declaration too many
/// only ever suppresses a finding.
/// Operation: nearest-wins fold over the marked declarations.
fn nearest_owners<'a>(
    marked: &'a [Marked<'a>],
    lines: &HashMap<String, HashSet<usize>>,
    marker: MarkerKind,
) -> Vec<(usize, &'a Marked<'a>)> {
    let mut owners: HashMap<(&str, usize), &Marked<'_>> = HashMap::new();
    marked
        .iter()
        .filter(|m| carries(m, marker))
        .filter_map(|m| marker_line(lines, m.file, m.line).map(|line| (m, line)))
        .for_each(|(m, line)| {
            let entry = owners.entry((m.file, line)).or_insert(m);
            if m.line < entry.line {
                *entry = m;
            }
        });
    let mut out: Vec<(usize, &Marked<'_>)> =
        owners.into_iter().map(|((_, line), m)| (line, m)).collect();
    out.sort_by_key(|(line, m)| (m.file, *line, m.line));
    out
}

/// Operation: marker → line index, no own calls.
fn lines_for<'a>(
    ctx: &'a MarkerContext<'a>,
    marker: MarkerKind,
) -> &'a HashMap<String, HashSet<usize>> {
    match marker {
        MarkerKind::TestHelper => ctx.test_helper_lines,
        _ => ctx.api_lines,
    }
}

/// Operation: flag lookup, no own calls.
fn carries(m: &Marked<'_>, marker: MarkerKind) -> bool {
    match marker {
        MarkerKind::TestHelper => m.is_test_helper,
        _ => m.is_api,
    }
}

/// Markers no declaration claims. Both markers only ever affect declaration-
/// level checks (DRY-002 for functions, DRY-006 for types and constants,
/// TQ-003), so one sitting on a `pub use` re-export, a module or a trait
/// provably changes nothing — and would otherwise stay an unverified silencer
/// forever.
///
/// Attachment mirrors the marking passes exactly (same annotation window): a
/// marker counts as claimed precisely when `mark_annotated` would have picked
/// it up, so this can never call a working marker unattached.
/// Operation: set difference over the claimed lines.
fn unattached_orphans(
    marked: &[Marked<'_>],
    lines: &HashMap<String, HashSet<usize>>,
    marker: MarkerKind,
) -> Vec<OrphanSuppression> {
    let what = marker_word(marker);
    // Only a declaration that actually carries THIS marker claims its line — one
    // marked `qual:api` must not make a neighbouring `qual:test_helper` look
    // attached.
    let claimed: HashSet<(&str, usize)> = marked
        .iter()
        .filter(|m| carries(m, marker))
        .filter_map(|m| marker_line(lines, m.file, m.line).map(|l| (m.file, l)))
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
            reason: Some(reason_for(Verdict::NotAttached, what, "", FUNCTION)),
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
    /// Attached, but that declaration is excluded from its checks anyway. The
    /// text names which exemption applies, since it differs by kind.
    NoEffectOnExempt { why: &'static str },
    /// `qual:api` on an item no outside consumer can name — the marker's
    /// premise is false. `called` refines the remedy; `None` when a name
    /// collision makes the call unattributable, which does not affect the
    /// verdict itself (reachability is decidable on its own).
    NeverApplied { called: Option<bool> },
    /// The marker was right once, but production calls the function now.
    Spent,
}

/// Decide whether a marked declaration must be reported, and why.
/// `qual:test_helper` is judged only by use: being unreachable from outside the
/// crate is its normal, intended state.
/// Integration: exemption check + reachability question.
fn classify(m: &Marked<'_>, reach: &ExternalReach, marker: MarkerKind) -> Option<Verdict> {
    // Excluded from its checks whatever the marker says, so the marker cannot
    // be doing anything here.
    if let Some(why) = m.exempt {
        return Some(Verdict::NoEffectOnExempt { why });
    }
    // Reachability is decidable on its own, so an unattributable use must not
    // hide a marker that could never have applied — it only blurs the remedy.
    if marker == MarkerKind::Api && !reach.is_externally_reachable(m.file, m.name) {
        return Some(Verdict::NeverApplied { called: m.used });
    }
    match m.used? {
        true => Some(Verdict::Spent),
        false => None,
    }
}

/// Whether production uses the declaration, or `None` when that cannot be
/// attributed.
///
/// Both evidence sets are keyed by bare name: the call collector records a path
/// call by its **last segment**, and the reference collector records every name
/// occurrence. If another declaration shares that name, the use cannot be
/// pinned to either — and claiming it would tell the author to delete a marker
/// that is still holding back a finding. A qualified `Type::method` match is
/// specific enough to attribute and always counts.
/// Operation: name lookup with an ambiguity gate.
fn attributed_use(
    qualified: &str,
    bare: &str,
    used_names: &HashSet<String>,
    ambiguous: &HashSet<&str>,
) -> Option<bool> {
    if qualified != bare && used_names.contains(qualified) {
        return Some(true);
    }
    if ambiguous.contains(bare) {
        return None;
    }
    Some(used_names.contains(bare))
}

/// Bare names declared more than once — a use recorded under such a name cannot
/// be attributed to one declaration. Functions and types share one namespace
/// here because both evidence sets are keyed by bare name, so a function called
/// `Entry` would otherwise vouch for a type called `Entry`.
/// Operation: frequency count over both declaration kinds.
fn ambiguous_names<'a>(ctx: &'a MarkerContext<'a>) -> HashSet<&'a str> {
    let names = ctx
        .declared_fns
        .iter()
        .map(|d| d.name.as_str())
        .chain(ctx.declared_types.iter().map(|d| d.name.as_str()));
    let mut seen: HashSet<&str> = HashSet::new();
    let mut twice: HashSet<&str> = HashSet::new();
    names.for_each(|n| {
        if !seen.insert(n) {
            twice.insert(n);
        }
    });
    twice
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

/// Operation: marker → its spelling, no own calls.
fn marker_word(marker: MarkerKind) -> &'static str {
    match marker {
        MarkerKind::TestHelper => "qual:test_helper",
        _ => "qual:api",
    }
}

/// Build the orphan finding for one stale marker.
/// Operation: struct construction, own calls hidden in the arguments.
fn orphan(m: &Marked<'_>, line: usize, marker: MarkerKind, verdict: Verdict) -> OrphanSuppression {
    OrphanSuppression {
        marker,
        file: m.file.to_string(),
        line,
        dimensions: Vec::new(),
        target: None,
        reason: Some(reason_for(verdict, marker_word(marker), m.display, m.kind)),
        kind: OrphanKind::Stale,
    }
}

/// The remedy for a marker that never applied. The shared half states the
/// cause; the tail depends on whether production uses the declaration — and
/// when a name collision leaves that open, says so instead of guessing.
/// Operation: message assembly, no own calls.
fn never_applied_reason(what: &str, name: &str, called: Option<bool>, kind: Kind) -> String {
    let (verb, finding) = (kind.verb, kind.finding);
    let head = format!(
        "{what} never applied here: {name} cannot be named from outside the crate \
         (it is not `pub`, or a module on its path is private)"
    );
    match called {
        Some(true) => format!("{head}, and production already {verb} it — remove the marker"),
        Some(false) => format!(
            "{head}, so there is no external consumer to excuse — use it from \
             production or delete it (removing the marker will surface the \
             {finding} finding)"
        ),
        None => format!(
            "{head} — remove the marker; another declaration shares this name, so \
             whatever the {finding} and untested checks then report is the real state"
        ),
    }
}

/// The remedy text for one verdict — each says what the author must do next,
/// and what will happen once they do it.
/// Operation: verdict → message, own call hidden in the arm.
fn reason_for(verdict: Verdict, what: &str, name: &str, kind: Kind) -> String {
    let (verb, finding) = (kind.verb, kind.finding);
    match verdict {
        Verdict::NotAttached => format!(
            "{what} is not attached to any declaration — it only affects the \
             declaration-level checks (dead code, dead types, untested), so on a \
             `pub use` re-export, a module or a trait it does nothing: remove it"
        ),
        Verdict::NoEffectOnExempt { why } => format!(
            "{what} changes nothing for {name}: it is already exempt from the \
             {finding} and untested checks ({why}) — remove the marker"
        ),
        Verdict::NeverApplied { called } => never_applied_reason(what, name, called, kind),
        Verdict::Spent => format!(
            "production {verb} {name}, so {what} excuses nothing — remove the marker \
             (if it is untested, an untested finding will surface: that is the point)"
        ),
    }
}
