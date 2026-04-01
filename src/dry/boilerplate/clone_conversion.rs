use super::{count_field_clones, BoilerplateFind};
use crate::config::sections::BoilerplateConfig;

/// Minimum cloned fields for clone-heavy conversion detection.
const MIN_CLONE_FIELDS: usize = 3;

/// Detect struct construction with many `.clone()` calls on fields.
/// Operation: body inspection logic; helper calls in closures.
// qual:allow(complexity) reason: "clone detection with closure-based body check"
pub(super) fn check_clone_heavy_conversion(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-008", config);
    let mut findings = Vec::new();
    for (file, _, syntax) in parsed {
        let check_body = |block: &syn::Block, line: usize| {
            for stmt in &block.stmts {
                let expr = match stmt {
                    syn::Stmt::Expr(e, _) => e,
                    syn::Stmt::Local(local) => {
                        if let Some(init) = &local.init {
                            &*init.expr
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                let clones = count_field_clones(expr);
                if clones >= MIN_CLONE_FIELDS {
                    return Some(BoilerplateFind {
                        pattern_id: "BP-008".to_string(),
                        file: file.clone(),
                        line,
                        struct_name: None,
                        description: format!(
                            "Struct construction with {clones} .clone() calls — consider Into/From or ownership transfer"
                        ),
                        suggestion:
                            "Consider implementing From/Into or restructuring to avoid cloning"
                                .to_string(),
                    });
                }
            }
            None
        };
        for item in &syntax.items {
            match item {
                syn::Item::Fn(f) => {
                    if let Some(finding) = check_body(&f.block, f.sig.ident.span().start().line) {
                        findings.push(finding);
                    }
                }
                syn::Item::Impl(imp) => {
                    for sub in &imp.items {
                        if let syn::ImplItem::Fn(m) = sub {
                            if let Some(finding) =
                                check_body(&m.block, m.sig.ident.span().start().line)
                            {
                                findings.push(finding);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    findings
}
