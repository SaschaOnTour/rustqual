//! HTML DRY section orchestrator. Projection (`build_dry_view`) lives
//! in `build`, rendering (`format_dry_section`) lives in `format`.

mod build;
mod format;

pub(super) use build::build_dry_view;
pub(super) use format::format_dry_section;
