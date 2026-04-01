use super::BoilerplateFind;
use crate::config::sections::BoilerplateConfig;
use std::collections::HashMap;
use syn::spanned::Spanned;
use syn::visit::Visit;

/// Minimum named fields per construction to be worth flagging.
const MIN_FIELDS: usize = 3;
/// Minimum same-type constructions in one function body.
const MIN_CONSTRUCTIONS: usize = 2;
/// Minimum field overlap ratio between any two constructions.
const MIN_OVERLAP_RATIO: f64 = 0.5;

struct Collector {
    entries: Vec<(String, Vec<String>, usize)>,
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        if node.rest.is_some() {
            syn::visit::visit_expr_struct(self, node);
            return;
        }
        let type_name = node
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        let fields: Vec<String> = node
            .fields
            .iter()
            .filter_map(|f| match &f.member {
                syn::Member::Named(ident) => Some(ident.to_string()),
                _ => None,
            })
            .collect();
        if fields.len() >= MIN_FIELDS {
            self.entries
                .push((type_name, fields, node.path.span().start().line));
        }
        syn::visit::visit_expr_struct(self, node);
    }
}

/// Detect functions with multiple struct constructions sharing fields.
/// Operation: block-level visitor + grouping + overlap analysis.
pub(super) fn check_repetitive_struct_update(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-009", config);

    let mut findings = Vec::new();
    let analyze = |block: &syn::Block, file: &str, fn_line: usize| -> Option<BoilerplateFind> {
        let mut c = Collector {
            entries: Vec::new(),
        };
        syn::visit::visit_block(&mut c, block);

        let mut by_type: HashMap<&str, Vec<&[String]>> = HashMap::new();
        c.entries
            .iter()
            .for_each(|(t, f, _)| by_type.entry(t.as_str()).or_default().push(f.as_slice()));

        by_type.into_iter().find_map(|(type_name, groups)| {
            if groups.len() < MIN_CONSTRUCTIONS {
                return None;
            }
            let a: std::collections::HashSet<&str> = groups[0].iter().map(|s| s.as_str()).collect();
            let b: std::collections::HashSet<&str> = groups[1].iter().map(|s| s.as_str()).collect();
            let overlap = a.intersection(&b).count();
            let min_len = a.len().min(b.len()).max(1);
            (overlap as f64 / min_len as f64 >= MIN_OVERLAP_RATIO).then(|| BoilerplateFind {
                pattern_id: "BP-009".to_string(),
                file: file.to_string(),
                line: fn_line,
                struct_name: Some(type_name.to_string()),
                description: format!(
                    "{} constructions of `{}` with overlapping fields",
                    groups.len(),
                    type_name
                ),
                suggestion: "Use struct update syntax: `Type { changed, ..base }`".to_string(),
            })
        })
    };

    parsed.iter().for_each(|(file, _, syntax)| {
        syntax.items.iter().for_each(|item| match item {
            syn::Item::Fn(f) => {
                if let Some(finding) = analyze(&f.block, file, f.sig.ident.span().start().line) {
                    findings.push(finding);
                }
            }
            syn::Item::Impl(imp) => imp.items.iter().for_each(|sub| {
                if let syn::ImplItem::Fn(m) = sub {
                    if let Some(finding) = analyze(&m.block, file, m.sig.ident.span().start().line)
                    {
                        findings.push(finding);
                    }
                }
            }),
            _ => {}
        });
    });
    findings
}
