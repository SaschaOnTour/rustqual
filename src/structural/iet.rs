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
        syn::Item::Mod(m) => {
            if !super::has_cfg_test_attr(&m.attrs) {
                m.content.iter().for_each(|(_, items)| {
                    items
                        .iter()
                        .for_each(|i| collect_item_errors(i, error_types));
                });
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn detect_in(source: &str) -> Vec<StructuralWarning> {
        let syntax = syn::parse_file(source).expect("test source");
        let parsed = vec![("lib.rs".to_string(), source.to_string(), syntax)];
        let config = StructuralConfig::default();
        let mut warnings = Vec::new();
        detect_iet(&mut warnings, &parsed, &config);
        warnings
    }

    #[test]
    fn test_consistent_error_types_not_flagged() {
        let w = detect_in("pub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> Result<i32, String> { Ok(1) }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_inconsistent_error_types_flagged() {
        let w = detect_in("pub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> Result<i32, std::io::Error> { Ok(1) }");
        assert_eq!(w.len(), 1);
        assert!(matches!(
            w[0].kind,
            StructuralWarningKind::InconsistentErrorTypes { .. }
        ));
    }

    #[test]
    fn test_single_pub_fn_not_flagged() {
        let w = detect_in("pub fn a() -> Result<(), String> { Ok(()) }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_private_fns_not_counted() {
        let w = detect_in("pub fn a() -> Result<(), String> { Ok(()) } fn b() -> Result<i32, std::io::Error> { Ok(1) }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_no_result_return_not_counted() {
        let w = detect_in("pub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> i32 { 1 }");
        assert!(w.is_empty());
    }

    #[test]
    fn test_normalized_paths() {
        // std::io::Error and io::Error should be the same
        let w = detect_in("pub fn a() -> Result<(), io::Error> { todo!() } pub fn b() -> Result<(), std::io::Error> { todo!() }");
        assert!(
            w.is_empty(),
            "std:: prefix should be stripped for comparison"
        );
    }

    #[test]
    fn test_disabled_check() {
        let syntax = syn::parse_file("pub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> Result<i32, std::io::Error> { Ok(1) }").expect("test source");
        let parsed = vec![("lib.rs".to_string(), String::new(), syntax)];
        let config = StructuralConfig {
            check_iet: false,
            ..StructuralConfig::default()
        };
        let mut warnings = Vec::new();
        detect_iet(&mut warnings, &parsed, &config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_cfg_test_module_excluded() {
        let w = detect_in("#[cfg(test)] mod tests { pub fn a() -> Result<(), String> { Ok(()) } pub fn b() -> Result<i32, std::io::Error> { Ok(1) } }");
        assert!(w.is_empty());
    }
}
