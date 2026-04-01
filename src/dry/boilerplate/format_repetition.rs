use super::BoilerplateFind;
use crate::config::sections::BoilerplateConfig;
use std::collections::HashMap;
use syn::spanned::Spanned;
use syn::visit::Visit;

/// Minimum identical format strings in one file to flag.
const MIN_REPETITIONS: usize = 3;
/// Minimum format string length (including quotes) to consider.
/// Excludes trivially simple pass-through strings like `"{}"`.
const MIN_FORMAT_STRING_LEN: usize = 5;
/// Macro names whose format strings are checked.
const FORMAT_MACROS: &[&str] = &["format", "println", "eprintln", "print", "write", "writeln"];

struct FormatCollector {
    formats: Vec<(String, usize)>,
}

impl<'ast> Visit<'ast> for FormatCollector {
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let name = node
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        if FORMAT_MACROS.contains(&name.as_str()) {
            let mut tokens = node.tokens.clone().into_iter();
            if let Some(proc_macro2::TokenTree::Literal(lit)) = tokens.next() {
                let s = lit.to_string();
                if s.starts_with('"') && s.len() >= MIN_FORMAT_STRING_LEN {
                    self.formats.push((s, node.path.span().start().line));
                }
            }
        }
    }
}

/// Detect files with many identical format!/println! format strings.
/// Operation: file-level visitor + grouping logic.
pub(super) fn check_format_repetition(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    pattern_guard!("BP-010", config);

    let mut findings = Vec::new();
    for (file, _, syntax) in parsed {
        let mut collector = FormatCollector {
            formats: Vec::new(),
        };
        syn::visit::visit_file(&mut collector, syntax);

        let mut by_format: HashMap<&str, Vec<usize>> = HashMap::new();
        collector
            .formats
            .iter()
            .for_each(|(fmt, line)| by_format.entry(fmt.as_str()).or_default().push(*line));

        by_format
            .iter()
            .filter(|(_, lines)| lines.len() >= MIN_REPETITIONS)
            .for_each(|(fmt_str, lines)| {
                findings.push(BoilerplateFind {
                    pattern_id: "BP-010".to_string(),
                    file: file.clone(),
                    line: lines[0],
                    struct_name: None,
                    description: format!(
                        "Format string {} repeated {} times",
                        fmt_str,
                        lines.len()
                    ),
                    suggestion: "Extract repeated format string into a helper function or constant"
                        .to_string(),
                });
            });
    }
    findings
}
