use std::collections::HashMap;

use syn::spanned::Spanned;
use syn::visit::Visit;

use crate::adapters::shared::file_visitor::{visit_all_files, FileVisitor};
use crate::config::sections::DuplicatesConfig;

/// Maximum entries per hash group before skipping pairwise comparison.
const MAX_WINDOW_GROUP_SIZE: usize = 50;

// ── Result types ────────────────────────────────────────────────

/// A group of matching code fragments across different functions.
#[derive(Debug, Clone)]
pub struct FragmentGroup {
    pub entries: Vec<FragmentEntry>,
    pub statement_count: usize,
    pub suppressed: bool,
}

/// An individual fragment location within a function.
#[derive(Debug, Clone)]
pub struct FragmentEntry {
    pub qualified_name: String,
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
}

// ── Internal types ──────────────────────────────────────────────

/// Metadata for a function whose body was scanned for fragments.
pub(crate) struct FnInfo {
    qualified_name: String,
    file: String,
    /// (start_line, end_line) for each top-level statement in the body.
    stmt_lines: Vec<(usize, usize)>,
}

/// A hashed window of consecutive statements within a function.
pub(crate) struct WindowEntry {
    fn_idx: usize,
    stmt_start: usize,
    hash: u64,
}

/// A matched pair of windows in two different functions.
pub(crate) struct PairMatch {
    fn_a: usize,
    fn_b: usize,
    stmt_a: usize,
    stmt_b: usize,
}

// ── Detection API ───────────────────────────────────────────────

/// Detect duplicate code fragments across parsed files.
/// Integration: orchestrates window collection, pair matching, and fragment merging.
pub fn detect_fragments(
    parsed: &[(String, String, syn::File)],
    config: &DuplicatesConfig,
) -> Vec<FragmentGroup> {
    let (fn_infos, windows) = collect_all_windows(parsed, config);
    let pairs = extract_matching_pairs(&windows);
    merge_into_fragments(pairs, &fn_infos, config.min_statements)
}

// ── Window collection ───────────────────────────────────────────

/// Collect all statement windows from all functions in parsed files.
/// Trivial: creates visitor and delegates to visit_all_files.
fn collect_all_windows(
    parsed: &[(String, String, syn::File)],
    config: &DuplicatesConfig,
) -> (Vec<FnInfo>, Vec<WindowEntry>) {
    let mut collector = FragmentCollector {
        config,
        file: String::new(),
        fn_infos: Vec::new(),
        windows: Vec::new(),
        parent_type: None,
        is_trait_impl: false,
    };
    visit_all_files(parsed, &mut collector);
    (collector.fn_infos, collector.windows)
}

// ── Pair matching ───────────────────────────────────────────────

/// Group windows by hash and extract cross-function matching pairs.
/// Operation: hash grouping + pair extraction logic, no own calls.
pub(crate) fn extract_matching_pairs(windows: &[WindowEntry]) -> Vec<PairMatch> {
    let mut by_hash: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, w) in windows.iter().enumerate() {
        by_hash.entry(w.hash).or_default().push(i);
    }

    let mut pairs = Vec::new();
    for indices in by_hash.values() {
        if indices.len() < 2 || indices.len() > MAX_WINDOW_GROUP_SIZE {
            continue;
        }
        for i in 0..indices.len() {
            for j in (i + 1)..indices.len() {
                let wa = &windows[indices[i]];
                let wb = &windows[indices[j]];
                if wa.fn_idx != wb.fn_idx {
                    pairs.push(PairMatch {
                        fn_a: wa.fn_idx,
                        fn_b: wb.fn_idx,
                        stmt_a: wa.stmt_start,
                        stmt_b: wb.stmt_start,
                    });
                }
            }
        }
    }
    pairs
}

// ── Fragment merging ────────────────────────────────────────────

/// Merge adjacent pair matches into maximal fragment groups.
/// Integration: canonicalise + sort, then merge each function-pair group.
pub(crate) fn merge_into_fragments(
    mut pairs: Vec<PairMatch>,
    fn_infos: &[FnInfo],
    window_size: usize,
) -> Vec<FragmentGroup> {
    if pairs.is_empty() {
        return vec![];
    }
    canonicalize_pairs(&mut pairs);
    pairs.sort_unstable_by_key(|p| (p.fn_a, p.fn_b, p.stmt_a, p.stmt_b));
    pairs.dedup_by_key(|p| (p.fn_a, p.fn_b, p.stmt_a, p.stmt_b));

    let mut result = Vec::new();
    let mut i = 0;
    while i < pairs.len() {
        let j = group_end(&pairs, i);
        let (fa, fb) = (pairs[i].fn_a, pairs[i].fn_b);
        result.extend(merge_group(&pairs[i..j], fa, fb, fn_infos, window_size));
        i = j;
    }
    result
}

/// Put the smaller fn index first in every pair (and swap its statements too).
/// Operation: in-place normalisation, std swaps only.
fn canonicalize_pairs(pairs: &mut [PairMatch]) {
    for p in pairs {
        if p.fn_a > p.fn_b {
            std::mem::swap(&mut p.fn_a, &mut p.fn_b);
            std::mem::swap(&mut p.stmt_a, &mut p.stmt_b);
        }
    }
}

/// Index one past the last pair sharing `pairs[i]`'s `(fn_a, fn_b)`.
/// Operation: forward scan, no own calls.
fn group_end(pairs: &[PairMatch], i: usize) -> usize {
    let (fa, fb) = (pairs[i].fn_a, pairs[i].fn_b);
    let mut j = i;
    while j < pairs.len() && pairs[j].fn_a == fa && pairs[j].fn_b == fb {
        j += 1;
    }
    j
}

/// Merge consecutive matches (both statement indices increment by 1) within one
/// function-pair group into maximal fragment groups.
/// Operation: run-length merge, find building in a helper.
fn merge_group(
    slice: &[PairMatch],
    fa: usize,
    fb: usize,
    fn_infos: &[FnInfo],
    window_size: usize,
) -> Vec<FragmentGroup> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < slice.len() {
        let end = run_end(slice, k);
        let stmt_count = end - k + window_size;
        out.push(make_fragment_group(
            &fn_infos[fa],
            &fn_infos[fb],
            slice[k].stmt_a,
            slice[k].stmt_b,
            stmt_count,
        ));
        k = end + 1;
    }
    out
}

/// Index of the last pair in the consecutive run starting at `k`.
/// Operation: forward scan, no own calls.
fn run_end(slice: &[PairMatch], k: usize) -> usize {
    let mut end = k;
    while end + 1 < slice.len()
        && slice[end + 1].stmt_a == slice[end].stmt_a + 1
        && slice[end + 1].stmt_b == slice[end].stmt_b + 1
    {
        end += 1;
    }
    end
}

/// Build a two-entry FragmentGroup for the merged run. The entries are
/// constructed inline (nested in the group) so the necessary owned copies of
/// each `FnInfo`'s strings stay a layer-boundary detail, not a flagged pattern.
/// Operation: line-span lookup + nested struct construction.
fn make_fragment_group(
    info_a: &FnInfo,
    info_b: &FnInfo,
    start_a: usize,
    start_b: usize,
    stmt_count: usize,
) -> FragmentGroup {
    let (a_start, a_end) = stmt_line_span(info_a, start_a, start_a + stmt_count - 1);
    let (b_start, b_end) = stmt_line_span(info_b, start_b, start_b + stmt_count - 1);
    FragmentGroup {
        entries: vec![
            FragmentEntry {
                qualified_name: info_a.qualified_name.clone(),
                file: info_a.file.clone(),
                start_line: a_start,
                end_line: a_end,
            },
            FragmentEntry {
                qualified_name: info_b.qualified_name.clone(),
                file: info_b.file.clone(),
                start_line: b_start,
                end_line: b_end,
            },
        ],
        statement_count: stmt_count,
        suppressed: false,
    }
}

/// Source `(start_line, end_line)` for statements `start..=end` of `info`.
/// Operation: two indexed lookups, no own calls.
fn stmt_line_span(info: &FnInfo, start: usize, end: usize) -> (usize, usize) {
    let line_start = info.stmt_lines.get(start).map_or(0, |l| l.0);
    let line_end = info.stmt_lines.get(end).map_or(line_start, |l| l.1);
    (line_start, line_end)
}

// ── FragmentCollector (AST visitor) ─────────────────────────────

/// AST visitor that collects statement windows from all function bodies.
struct FragmentCollector<'a> {
    config: &'a DuplicatesConfig,
    file: String,
    fn_infos: Vec<FnInfo>,
    windows: Vec<WindowEntry>,
    parent_type: Option<String>,
    is_trait_impl: bool,
}

impl FileVisitor for FragmentCollector<'_> {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
        self.parent_type = None;
        self.is_trait_impl = false;
    }
}

impl FragmentCollector<'_> {
    /// Process a function body: record fn_info and extract statement windows.
    /// Operation: window extraction logic; normalize/hash calls hidden in closure.
    fn process_body(&mut self, name: &str, body: &syn::Block) {
        if self.config.ignore_trait_impls && self.is_trait_impl {
            return;
        }

        let window_size = self.config.min_statements;
        if body.stmts.len() < window_size {
            return;
        }

        let stmt_lines: Vec<(usize, usize)> = body
            .stmts
            .iter()
            .map(|s| (s.span().start().line, s.span().end().line))
            .collect();

        let qualified_name = self
            .parent_type
            .as_ref()
            .map(|p| format!("{p}::{name}"))
            .unwrap_or_else(|| name.to_string());

        let fn_idx = self.fn_infos.len();
        self.fn_infos.push(FnInfo {
            qualified_name,
            file: self.file.clone(),
            stmt_lines,
        });

        // Closure hides own calls to normalize_stmts/structural_hash (lenient mode)
        let compute_hash = |stmts: &[syn::Stmt]| {
            let tokens = crate::adapters::shared::normalize::normalize_stmts(stmts);
            let hash = crate::adapters::shared::normalize::structural_hash(&tokens);
            (tokens.len(), hash)
        };

        let min_tokens = self.config.min_tokens;
        for i in 0..=body.stmts.len() - window_size {
            let window_stmts = &body.stmts[i..i + window_size];
            let (token_count, hash) = compute_hash(window_stmts);
            if token_count >= min_tokens {
                self.windows.push(WindowEntry {
                    fn_idx,
                    stmt_start: i,
                    hash,
                });
            }
        }
    }
}

impl<'ast> Visit<'ast> for FragmentCollector<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let name = node.sig.ident.to_string();
        self.process_body(&name, &node.block);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let prev_parent = self.parent_type.take();
        let prev_is_trait = self.is_trait_impl;

        self.is_trait_impl = node.trait_.is_some();
        if let syn::Type::Path(tp) = &*node.self_ty {
            if let Some(seg) = tp.path.segments.last() {
                self.parent_type = Some(seg.ident.to_string());
            }
        }

        syn::visit::visit_item_impl(self, node);

        self.parent_type = prev_parent;
        self.is_trait_impl = prev_is_trait;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let name = node.sig.ident.to_string();
        self.process_body(&name, &node.block);
    }
}
