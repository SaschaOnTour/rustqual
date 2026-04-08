use crate::findings::Suppression;

/// Trait for DRY finding groups that can be suppressed.
pub(crate) trait DrySuppressible {
    fn set_suppressed(&mut self, val: bool);
    fn entry_locations(&self) -> Vec<(&str, usize)>;
}

impl DrySuppressible for crate::dry::functions::DuplicateGroup {
    fn set_suppressed(&mut self, val: bool) {
        self.suppressed = val;
    }
    fn entry_locations(&self) -> Vec<(&str, usize)> {
        self.entries
            .iter()
            .map(|e| (e.file.as_str(), e.line))
            .collect()
    }
}

impl DrySuppressible for crate::dry::match_patterns::RepeatedMatchGroup {
    fn set_suppressed(&mut self, val: bool) {
        self.suppressed = val;
    }
    fn entry_locations(&self) -> Vec<(&str, usize)> {
        self.entries
            .iter()
            .map(|e| (e.file.as_str(), e.line))
            .collect()
    }
}

impl DrySuppressible for crate::dry::fragments::FragmentGroup {
    fn set_suppressed(&mut self, val: bool) {
        self.suppressed = val;
    }
    fn entry_locations(&self) -> Vec<(&str, usize)> {
        self.entries
            .iter()
            .map(|e| (e.file.as_str(), e.start_line))
            .collect()
    }
}

/// Mark DRY finding groups as suppressed when any entry has `// qual:allow(dry)`.
/// Operation: iterates groups checking entries against suppression lines.
pub(crate) fn mark_dry_suppressions<T: DrySuppressible>(
    groups: &mut [T],
    suppression_lines: &std::collections::HashMap<String, Vec<Suppression>>,
) {
    let dry_dim = crate::findings::Dimension::Dry;
    let window = crate::findings::ANNOTATION_WINDOW;
    groups.iter_mut().for_each(|g| {
        let suppressed = g.entry_locations().iter().any(|(file, line)| {
            suppression_lines
                .get(*file)
                .map(|sups| {
                    sups.iter().any(|sup| {
                        let in_window = sup.line <= *line && line - sup.line <= window;
                        in_window && sup.covers(dry_dim)
                    })
                })
                .unwrap_or(false)
        });
        g.set_suppressed(suppressed);
    });
}

/// Mark duplicate groups as suppressed when members are `// qual:inverse(fn)` pairs.
/// Operation: iterates groups checking entries against inverse annotation lines.
pub(super) fn mark_inverse_suppressions(
    groups: &mut [crate::dry::functions::DuplicateGroup],
    inverse_lines: &std::collections::HashMap<String, Vec<(usize, String)>>,
) {
    let window = crate::findings::ANNOTATION_WINDOW;
    groups.iter_mut().filter(|g| !g.suppressed).for_each(|g| {
        g.suppressed = g.entries.iter().any(|entry| {
            inverse_lines
                .get(&entry.file)
                .map(|inv| {
                    inv.iter().any(|(line, target)| {
                        let in_window = *line <= entry.line && entry.line - line <= window;
                        in_window
                            && g.entries.iter().any(|other| {
                                other.name == *target || other.qualified_name == *target
                            })
                    })
                })
                .unwrap_or(false)
        });
    });
}
