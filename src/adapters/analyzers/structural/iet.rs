use std::collections::HashSet;

use crate::config::StructuralConfig;
use crate::findings::Dimension;

use super::{StructuralWarning, StructuralWarningKind};

/// Detect inconsistent error types: pub fns in same module return different Result<_, E>.
/// Operation: collects return types per file, flags inconsistencies.
pub(crate) fn detect_iet(
    warnings: &mut Vec<StructuralWarning>,
    parsed: &[(String, String, syn::File)],
    config: &StructuralConfig,
) {
    if !config.check_iet {
        return;
    }
    parsed.iter().for_each(|(path, _, syntax)| {
        let error_types = collect_pub_error_types(syntax);
        if error_types.len() >= 2 {
            let mut types: Vec<String> = error_types.into_iter().collect();
            types.sort();
            warnings.push(StructuralWarning {
                file: path.clone(),
                line: 1,
                name: path.clone(),
                kind: StructuralWarningKind::InconsistentErrorTypes { error_types: types },
                dimension: Dimension::Coupling,
                suppressed: false,
            });
        }
    });
}

/// Collect distinct error types from pub fn return types in a file.
/// Operation: iterates items, extracts Result<_, E> error types.
fn collect_pub_error_types(file: &syn::File) -> HashSet<String> {
    let mut error_types = HashSet::new();
    file.items.iter().for_each(|item| {
        collect_item_errors(item, &mut error_types);
    });
    error_types
}

/// Extract error types from a single item (recurse into non-test modules).
/// Operation: match dispatch + return type extraction, own calls hidden in closures.
fn collect_item_errors(item: &syn::Item, error_types: &mut HashSet<String>) {
    let extract_error = |output: &syn::ReturnType| extract_result_error_type(output);
    match item {
        syn::Item::Fn(f) => {
            if matches!(f.vis, syn::Visibility::Public(_)) {
                extract_error(&f.sig.output).iter().for_each(|e| {
                    error_types.insert(e.clone());
                });
            }
        }
        syn::Item::Mod(m) if !super::has_cfg_test_attr(&m.attrs) => {
            m.content.iter().for_each(|(_, items)| {
                items
                    .iter()
                    .for_each(|i| collect_item_errors(i, error_types));
            });
        }
        _ => {}
    }
}

/// Extract the error type from a fn return type if it's `Result<T, E>`.
/// Operation: type path parsing + normalization inlined.
fn extract_result_error_type(output: &syn::ReturnType) -> Option<String> {
    let ty = match output {
        syn::ReturnType::Type(_, ty) => ty,
        syn::ReturnType::Default => return None,
    };
    let path = match ty.as_ref() {
        syn::Type::Path(tp) => &tp.path,
        _ => return None,
    };
    let last_seg = path.segments.last()?;
    if last_seg.ident != "Result" {
        return None;
    }
    let args = match &last_seg.arguments {
        syn::PathArguments::AngleBracketed(a) => a,
        _ => return None,
    };
    // Result<T, E> — the E is the second generic arg
    let error_arg = args.args.iter().nth(1)?;
    // Normalize: strip spaces, remove std::/core:: prefix
    let full = quote::quote!(#error_arg).to_string();
    let normalized = full.replace(' ', "");
    let stripped = normalized
        .strip_prefix("std::")
        .or_else(|| normalized.strip_prefix("core::"))
        .unwrap_or(&normalized);
    Some(stripped.to_string())
}
