use std::collections::HashMap;

use syn::visit::Visit;

use crate::config::sections::SrpConfig;

use super::union_find::UnionFind;
use super::ModuleSrpWarning;

/// Information about a free (non-method) function collected from the AST.
pub(crate) struct FreeFunctionInfo {
    pub(crate) name: String,
    pub(crate) is_private: bool,
    pub(crate) statement_count: usize,
}

/// AST visitor that collects free function metadata for cohesion analysis.
struct FreeFunctionCollector<'a> {
    functions: &'a mut Vec<FreeFunctionInfo>,
}

impl<'ast, 'a> Visit<'ast> for FreeFunctionCollector<'a> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.functions.push(FreeFunctionInfo {
            name: node.sig.ident.to_string(),
            is_private: matches!(node.vis, syn::Visibility::Inherited),
            statement_count: node.block.stmts.len(),
        });
        // Don't recurse into body — we only need function-level info
    }

    // Don't descend into impl blocks or nested modules
    fn visit_item_impl(&mut self, _node: &'ast syn::ItemImpl) {}
    fn visit_item_mod(&mut self, _node: &'ast syn::ItemMod) {}
    fn visit_item_trait(&mut self, _node: &'ast syn::ItemTrait) {}
}

/// Collect free functions from a parsed syntax tree.
/// Operation: creates visitor and walks items.
pub(crate) fn collect_free_functions(syntax: &syn::File) -> Vec<FreeFunctionInfo> {
    let mut functions = Vec::new();
    let mut collector = FreeFunctionCollector {
        functions: &mut functions,
    };
    collector.visit_file(syntax);
    functions
}

/// Count independent function clusters in a file using Union-Find.
/// Operation: Union-Find on private substantive functions using call graph.
pub(crate) fn count_independent_clusters(
    fn_info: &[FreeFunctionInfo],
    call_graph: &[(String, Vec<String>)],
    min_statements: usize,
) -> (usize, Vec<Vec<String>>) {
    let substantive: Vec<&FreeFunctionInfo> = fn_info
        .iter()
        .filter(|f| f.is_private && f.statement_count >= min_statements)
        .collect();
    if substantive.is_empty() {
        return (0, vec![]);
    }
    let name_to_idx: HashMap<&str, usize> = substantive
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();
    let make_uf = |size| UnionFind::new(size);
    let mut uf = make_uf(substantive.len());
    let unite = |uf: &mut UnionFind, a: usize, b: usize| uf.union(a, b);
    let components = |uf: &mut UnionFind| uf.component_members();
    // Union-Find: unite private targets per caller + connect caller if private
    call_graph.iter().for_each(|(fn_name, targets)| {
        let private_targets: Vec<usize> = targets
            .iter()
            .filter_map(|t| name_to_idx.get(t.as_str()).copied())
            .collect();
        (1..private_targets.len()).for_each(|i| {
            unite(&mut uf, private_targets[0], private_targets[i]);
        });
        if let Some(&caller_idx) = name_to_idx.get(fn_name.as_str()) {
            if let Some(&first) = private_targets.first() {
                unite(&mut uf, caller_idx, first);
            }
        }
    });
    let component_members = components(&mut uf);
    let mut cluster_names: Vec<Vec<String>> = component_members
        .values()
        .map(|indices| {
            indices
                .iter()
                .map(|&i| substantive[i].name.clone())
                .collect()
        })
        .collect();
    cluster_names.iter_mut().for_each(|c| c.sort());
    cluster_names.sort();
    let count = cluster_names.len();
    (count, cluster_names)
}

/// Analyze module-level SRP: flag files with excessive production line counts
/// or too many independent function clusters.
/// Operation: iterates files, computes production lines, length score,
/// and independent clusters via closures.
pub fn analyze_module_srp(
    parsed: &[(String, String, syn::File)],
    config: &SrpConfig,
    file_call_graph: &HashMap<String, Vec<(String, Vec<String>)>>,
    cfg_test_files: &std::collections::HashSet<String>,
) -> Vec<ModuleSrpWarning> {
    parsed
        .iter()
        .filter(|(path, _, _)| !cfg_test_files.contains(path))
        .filter_map(|(path, source, syntax)| {
            let production_lines = count_production_lines(source);
            let score = compute_file_length_score(
                production_lines,
                config.file_length_baseline,
                config.file_length_ceiling,
            );

            let free_fns = collect_free_functions(syntax);
            let call_graph = file_call_graph
                .get(path)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let (cluster_count, cluster_names) =
                count_independent_clusters(&free_fns, call_graph, config.min_cluster_statements);

            let has_length_warning = score > 0.0;
            // Use strict `>` for consistency with the other `max_*`
            // thresholds in this crate (max_cognitive, max_fan_in,
            // max_function_lines etc. all treat the configured value
            // as the highest allowed, warning only when exceeded).
            let has_cohesion_warning = cluster_count > config.max_independent_clusters;

            if has_length_warning || has_cohesion_warning {
                Some(ModuleSrpWarning {
                    module: path.clone(),
                    file: path.clone(),
                    production_lines,
                    length_score: score,
                    independent_clusters: cluster_count,
                    cluster_names,
                    suppressed: false,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Count production lines: lines from start of file to first
/// `#[cfg(test)]` attribute. Stops on any line that begins with
/// `#[cfg(test)]` so both the multi-line form
/// (`#[cfg(test)]\nmod tests { … }`) and the single-line form
/// (`#[cfg(test)] mod tests { … }`) are handled. Blank lines, `//`
/// line comments, and the body of `/* … */` block comments (including
/// their opening / closing lines) do not count.
/// Operation: per-line classification with a block-comment state flag.
pub(crate) fn count_production_lines(source: &str) -> usize {
    let mut count = 0;
    let mut in_block_comment = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if !in_block_comment && trimmed.starts_with("#[cfg(test)]") {
            break;
        }
        if is_noise_line(trimmed, &mut in_block_comment) {
            continue;
        }
        count += 1;
    }
    count
}

/// Classify a trimmed line as non-production (blank / comment) vs code.
/// Scans left-to-right, tracking multi-line `/* … */` state through
/// `in_block_comment`. Correctly handles mid-line comments:
/// `let x = 1; /* note */` counts as code, `/* note */ let x = 1;`
/// also counts as code (unlike a leading-only heuristic), and
/// `/* note */` alone counts as a comment. Lines inside a block
/// comment that don't close it are pure comment.
/// Operation: char-by-char scan with a block-comment state flag.
fn is_noise_line(trimmed: &str, in_block_comment: &mut bool) -> bool {
    if trimmed.is_empty() {
        return true;
    }
    let mut has_code = false;
    let mut chars = trimmed.chars().peekable();
    while let Some(c) = chars.next() {
        if *in_block_comment {
            handle_in_comment(c, &mut chars, in_block_comment);
            continue;
        }
        match (c, chars.peek().copied()) {
            ('/', Some('/')) => return !has_code,
            ('/', Some('*')) => {
                chars.next();
                *in_block_comment = true;
            }
            _ if !c.is_whitespace() => has_code = true,
            _ => {}
        }
    }
    !has_code
}

/// When inside a block comment, consume chars until we see `*/`,
/// then clear the flag. Everything before is thrown away.
/// Operation: two-char lookahead to close `*/`.
fn handle_in_comment(
    c: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    in_block_comment: &mut bool,
) {
    if c == '*' && chars.peek() == Some(&'/') {
        chars.next();
        *in_block_comment = false;
    }
}

/// Compute file length penalty score.
/// Returns 0.0 below baseline, 1.0 above ceiling, linear between.
/// Operation: arithmetic.
pub(crate) fn compute_file_length_score(
    production_lines: usize,
    baseline: usize,
    ceiling: usize,
) -> f64 {
    // Misconfiguration guard: if the thresholds are inverted the
    // subtraction below would underflow (usize). Handle this first so
    // the behaviour is consistent regardless of `production_lines`.
    if ceiling <= baseline {
        return 1.0;
    }
    if production_lines <= baseline {
        return 0.0;
    }
    if production_lines >= ceiling {
        return 1.0;
    }
    let range = (ceiling - baseline) as f64;
    (production_lines - baseline) as f64 / range
}
