//! Projection of `&[DryFinding]` into the typed `HtmlDryView`.

use crate::adapters::report::html::views::HtmlDryView;
use crate::adapters::report::projections::dry::split_dry_findings;
use crate::domain::findings::DryFinding;

/// Project DRY findings into the typed view via the shared splitter.
pub(crate) fn build_dry_view(findings: &[DryFinding]) -> HtmlDryView {
    let buckets = split_dry_findings(findings);
    HtmlDryView {
        duplicate_groups: buckets.duplicate_groups,
        fragment_groups: buckets.fragment_groups,
        repeated_match_groups: buckets.repeated_match_groups,
        dead_code: buckets.dead_code,
        boilerplate: buckets.boilerplate,
        wildcards: buckets.wildcards,
    }
}
