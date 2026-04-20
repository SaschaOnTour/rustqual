use crate::config::sections::BoilerplateConfig;

// ── Result type ────────────────────────────────────────────────

/// A boilerplate pattern finding.
#[derive(Debug, Clone)]
pub struct BoilerplateFind {
    pub pattern_id: String,
    pub file: String,
    pub line: usize,
    pub struct_name: Option<String>,
    pub description: String,
    pub suggestion: String,
    pub suppressed: bool,
}

// ── Pattern enablement macro ───────────────────────────────────

/// Early-return if the given pattern ID is disabled in config.
macro_rules! pattern_guard {
    ($id:expr, $config:expr) => {
        if !$config.patterns.is_empty() && $config.patterns.iter().all(|p| p != $id) {
            return vec![];
        }
    };
}

// ── Helpers (called only from within closures for IOSP) ────────

pub(crate) fn trait_name_of(imp: &syn::ItemImpl) -> Option<String> {
    imp.trait_
        .as_ref()
        .and_then(|(_, path, _)| path.segments.last().map(|s| s.ident.to_string()))
}

pub(crate) fn self_type_of(imp: &syn::ItemImpl) -> Option<String> {
    if let syn::Type::Path(tp) = &*imp.self_ty {
        tp.path.segments.last().map(|s| s.ident.to_string())
    } else {
        None
    }
}

pub(crate) fn single_return_expr(block: &syn::Block) -> Option<&syn::Expr> {
    if block.stmts.len() == 1 {
        if let syn::Stmt::Expr(expr, None) = &block.stmts[0] {
            return Some(expr);
        }
    }
    None
}

pub(crate) fn is_self_field_access(expr: &syn::Expr) -> bool {
    if let syn::Expr::Field(f) = expr {
        if let syn::Expr::Path(p) = &*f.base {
            return p.path.segments.last().is_some_and(|s| s.ident == "self");
        }
    }
    false
}

pub(crate) fn is_default_value_expr(expr: &syn::Expr) -> bool {
    match expr {
        syn::Expr::Lit(lit) => match &lit.lit {
            syn::Lit::Int(i) => i.base10_parse::<i64>().ok() == Some(0),
            syn::Lit::Float(f) => f.base10_parse::<f64>().ok() == Some(0.0),
            syn::Lit::Bool(b) => !b.value,
            syn::Lit::Str(s) => s.value().is_empty(),
            _ => false,
        },
        syn::Expr::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "None"),
        syn::Expr::Call(call) => {
            if let syn::Expr::Path(p) = &*call.func {
                let segs: Vec<_> = p
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect();
                matches!(
                    segs.iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .as_slice(),
                    ["Default", "default"]
                        | ["String", "new"]
                        | ["Vec", "new"]
                        | ["HashMap", "new"]
                        | ["HashSet", "new"]
                        | ["BTreeMap", "new"]
                        | ["BTreeSet", "new"]
                )
            } else {
                false
            }
        }
        syn::Expr::Macro(m) => {
            let name = m
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            name == "vec" && m.mac.tokens.is_empty()
        }
        _ => false,
    }
}

/// Check if a match arm pattern is a simple enum variant pattern suitable for
/// repetitive enum mapping detection. Accepts unit variants (`Color::Red`) and
/// tuple-struct variants with only wildcard sub-patterns (`Action::Add(_)`).
/// Rejects or-patterns, top-level wildcards, tuple patterns, and variable bindings.
/// Operation: pattern matching, no own calls.
fn is_simple_enum_pattern(pat: &syn::Pat) -> bool {
    match pat {
        // Unit variant: `Color::Red`
        syn::Pat::Path(_) => true,
        // Tuple-struct variant: `Action::Add(_)` — only if all sub-patterns are wildcards
        syn::Pat::TupleStruct(ts) => ts.elems.iter().all(|p| matches!(p, syn::Pat::Wild(_))),
        // Struct variant: `Msg { .. }` — only if fields are empty (rest-only)
        syn::Pat::Struct(ps) => ps.fields.is_empty(),
        _ => false,
    }
}

/// Check if all match arms represent a repetitive enum-to-enum mapping.
/// Arms must have simple enum variant patterns (no or-patterns, wildcards, bindings)
/// and path or call bodies. No guards allowed.
/// Operation: iterates arms checking pattern + body constraints.
pub(crate) fn is_repetitive_enum_mapping(arms: &[syn::Arm]) -> bool {
    arms.iter().all(|arm| {
        // Guard expressions disqualify
        if arm.guard.is_some() {
            return false;
        }
        // Pattern must be a simple enum variant
        is_simple_enum_pattern(&arm.pat)
            // Body must be a path expression (enum variant) or call
            && matches!(&*arm.body, syn::Expr::Path(_) | syn::Expr::Call(_))
    })
}

pub(crate) fn count_field_clones(expr: &syn::Expr) -> usize {
    if let syn::Expr::Struct(s) = expr {
        s.fields
            .iter()
            .filter(|f| {
                matches!(&f.expr, syn::Expr::MethodCall(mc) if mc.method == "clone" && mc.args.is_empty())
            })
            .count()
    } else {
        0
    }
}

// ── Pattern modules ────────────────────────────────────────────

mod builder;
mod clone_conversion;
mod error_enum;
mod format_repetition;
mod getter_setter;
mod manual_default;
mod repetitive_match;
mod struct_update;
mod trivial_display;
mod trivial_from;

// ── Detection API ──────────────────────────────────────────────

/// Detect boilerplate patterns across parsed files.
/// Integration: orchestrates all 10 pattern checkers.
pub fn detect_boilerplate(
    parsed: &[(String, String, syn::File)],
    config: &BoilerplateConfig,
) -> Vec<BoilerplateFind> {
    let mut findings = trivial_from::check_trivial_from(parsed, config);
    findings.extend(trivial_display::check_trivial_display(parsed, config));
    findings.extend(getter_setter::check_manual_getter_setter(parsed, config));
    findings.extend(builder::check_builder_boilerplate(parsed, config));
    findings.extend(manual_default::check_manual_default(parsed, config));
    findings.extend(repetitive_match::check_repetitive_match(parsed, config));
    findings.extend(error_enum::check_error_enum_boilerplate(parsed, config));
    findings.extend(clone_conversion::check_clone_heavy_conversion(
        parsed, config,
    ));
    findings.extend(struct_update::check_repetitive_struct_update(
        parsed, config,
    ));
    findings.extend(format_repetition::check_format_repetition(parsed, config));
    findings
}

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
