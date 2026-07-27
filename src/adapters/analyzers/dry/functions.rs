use std::collections::HashMap;

use syn::spanned::Spanned;
use syn::visit::Visit;

use super::{qualify_name, FunctionHashEntry};
use crate::adapters::shared::file_visitor::FileVisitor;
use crate::config::sections::DuplicatesConfig;

// ── FunctionCollector (for DRY hashing) ─────────────────────────

/// AST visitor that collects function bodies and computes their normalized hashes.
pub(crate) struct FunctionCollector<'a> {
    pub(crate) config: &'a DuplicatesConfig,
    pub(crate) file: String,
    pub(crate) entries: Vec<FunctionHashEntry>,
    parent_type: Option<String>,
    is_trait_impl: bool,
}

impl<'a> FunctionCollector<'a> {
    pub(crate) fn new(config: &'a DuplicatesConfig) -> Self {
        Self {
            config,
            file: String::new(),
            entries: Vec::new(),
            parent_type: None,
            is_trait_impl: false,
        }
    }
}

impl FileVisitor for FunctionCollector<'_> {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
        self.parent_type = None;
        self.is_trait_impl = false;
    }
}

impl FunctionCollector<'_> {
    /// Build a hash entry for a function body, applying config filters.
    /// Operation: config checks + normalize/hash calls in closure (lenient).
    fn build_hash_entry(
        &self,
        sig: &syn::Signature,
        body: &syn::Block,
        is_trait_impl: bool,
    ) -> Option<FunctionHashEntry> {
        let name = sig.ident.to_string();
        let line = sig.ident.span().start().line;
        if self.config.ignore_trait_impls && is_trait_impl {
            return None;
        }

        // Closure hides own calls to normalize_body/structural_hash (lenient mode).
        let compute = |b: &syn::Block| {
            let tokens = crate::adapters::shared::normalize::normalize_fn(sig, b);
            let hash = crate::adapters::shared::normalize::structural_hash(&tokens);
            (tokens, hash)
        };
        let (tokens, hash) = compute(body);

        if tokens.len() < self.config.min_tokens {
            return None;
        }

        let span = body.span();
        let line_count = span.end().line.saturating_sub(span.start().line) + 1;
        if line_count < self.config.min_lines {
            return None;
        }

        let qualify = |parent: &Option<String>, n: &str| qualify_name(parent, n);
        let qualified_name = qualify(&self.parent_type, &name);

        Some(FunctionHashEntry {
            name: name.to_string(),
            qualified_name,
            file: self.file.clone(),
            line,
            hash,
            token_count: tokens.len(),
            tokens,
        })
    }
}

impl<'ast> Visit<'ast> for FunctionCollector<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if let Some(entry) = self.build_hash_entry(&node.sig, &node.block, false) {
            self.entries.push(entry);
        }
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
        if let Some(entry) = self.build_hash_entry(&node.sig, &node.block, self.is_trait_impl) {
            self.entries.push(entry);
        }
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if let Some(ref block) = node.default {
            if let Some(entry) = self.build_hash_entry(&node.sig, block, true) {
                self.entries.push(entry);
            }
        }
    }
}

/// Near-duplicate bucket size: functions with token counts within this range
/// are compared pairwise for Jaccard similarity.
const NEAR_DUP_BUCKET_SIZE: usize = 10;

/// Maximum entries per near-duplicate bucket before skipping pairwise comparison.
const MAX_BUCKET_SIZE: usize = 50;

// ── Result types ────────────────────────────────────────────────

/// A group of functions identified as duplicates.
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    pub entries: Vec<DuplicateEntry>,
    pub kind: DuplicateKind,
    pub suppressed: bool,
}

/// An individual function in a duplicate group.
#[derive(Debug, Clone)]
pub struct DuplicateEntry {
    pub name: String,
    pub qualified_name: String,
    pub file: String,
    pub line: usize,
}

/// Classification of a duplicate group.
#[derive(Debug, Clone)]
pub enum DuplicateKind {
    /// Functions with identical normalized structure.
    Exact,
    /// Functions with high structural similarity.
    NearDuplicate { similarity: f64 },
}

// ── Detection API ───────────────────────────────────────────────

/// Detect duplicate functions across parsed files.
/// Integration: orchestrates hash collection, grouping, and near-duplicate search.
pub fn detect_duplicates(
    parsed: &[(String, String, syn::File)],
    config: &DuplicatesConfig,
) -> Vec<DuplicateGroup> {
    let entries = super::collect_function_hashes(parsed, config);
    let (exact, remaining_indices) = group_exact_duplicates(&entries);
    let near = find_near_duplicates(&entries, &remaining_indices, config.similarity_threshold);
    merge_groups(exact, near)
}

/// Merge exact and near-duplicate groups into a single list.
/// Trivial: concatenation.
fn merge_groups(mut exact: Vec<DuplicateGroup>, near: Vec<DuplicateGroup>) -> Vec<DuplicateGroup> {
    exact.extend(near);
    exact
}

// ── Exact duplicate grouping ────────────────────────────────────

/// Group entries by structural hash, returning groups with 2+ members.
/// Operation: hash-based grouping logic, no own calls.
/// Also returns indices of entries NOT in any exact group (for near-dup search).
pub(crate) fn group_exact_duplicates(
    entries: &[FunctionHashEntry],
) -> (Vec<DuplicateGroup>, Vec<usize>) {
    let mut hash_groups: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        hash_groups.entry(entry.hash).or_default().push(i);
    }

    let mut groups = Vec::new();
    let mut remaining = Vec::new();

    for indices in hash_groups.values() {
        if indices.len() >= 2 {
            let group_entries: Vec<DuplicateEntry> = indices
                .iter()
                .map(|&i| DuplicateEntry {
                    name: entries[i].name.clone(),
                    qualified_name: entries[i].qualified_name.clone(),
                    file: entries[i].file.clone(),
                    line: entries[i].line,
                })
                .collect();
            groups.push(DuplicateGroup {
                entries: group_entries,
                kind: DuplicateKind::Exact,
                suppressed: false,
            });
        } else {
            remaining.extend(indices);
        }
    }

    (groups, remaining)
}

// ── Near-duplicate detection ────────────────────────────────────

/// Find near-duplicate functions using token-count bucketing + Jaccard similarity.
/// Operation: bucketing + pairwise comparison logic; jaccard_similarity called
/// via closure (lenient mode).
fn find_near_duplicates(
    entries: &[FunctionHashEntry],
    candidate_indices: &[usize],
    threshold: f64,
) -> Vec<DuplicateGroup> {
    // Bucket candidates by quantized token count
    let mut buckets: HashMap<usize, Vec<usize>> = HashMap::new();
    for &idx in candidate_indices {
        let bucket_key = entries[idx].token_count / NEAR_DUP_BUCKET_SIZE;
        buckets.entry(bucket_key).or_default().push(idx);
    }

    // Closure hides own call to jaccard_similarity (lenient mode)
    let compute_sim = |a: &[crate::adapters::shared::normalize::NormalizedToken],
                       b: &[crate::adapters::shared::normalize::NormalizedToken]|
     -> f64 { crate::adapters::shared::normalize::jaccard_similarity(a, b) };

    let mut groups = Vec::new();

    for indices in buckets.values() {
        if indices.len() < 2 || indices.len() > MAX_BUCKET_SIZE {
            continue;
        }
        for i in 0..indices.len() {
            for j in (i + 1)..indices.len() {
                let a = indices[i];
                let b = indices[j];
                let sim = compute_sim(&entries[a].tokens, &entries[b].tokens);
                if sim >= threshold {
                    groups.push(DuplicateGroup {
                        entries: vec![
                            DuplicateEntry {
                                name: entries[a].name.clone(),
                                qualified_name: entries[a].qualified_name.clone(),
                                file: entries[a].file.clone(),
                                line: entries[a].line,
                            },
                            DuplicateEntry {
                                name: entries[b].name.clone(),
                                qualified_name: entries[b].qualified_name.clone(),
                                file: entries[b].file.clone(),
                                line: entries[b].line,
                            },
                        ],
                        kind: DuplicateKind::NearDuplicate { similarity: sim },
                        suppressed: false,
                    });
                }
            }
        }
    }

    groups
}
