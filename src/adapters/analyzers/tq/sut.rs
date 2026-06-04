use std::collections::HashSet;

use crate::adapters::shared::declared_function::DeclaredFunction;
use crate::adapters::shared::project_scope::ProjectScope;
use crate::adapters::shared::test_references::collect_test_references;

use super::{TqWarning, TqWarningKind};

/// Detect test functions that do not call any production function (TQ-002).
/// Operation: iterates test functions, compares call targets against known prod functions.
pub(crate) fn detect_no_sut_tests(
    parsed: &[(String, String, syn::File)],
    scope: &ProjectScope,
    declared_fns: &[DeclaredFunction],
    reaches_prod: &HashSet<String>,
) -> Vec<TqWarning> {
    // Build set of known production function names
    let prod_fn_names: HashSet<&str> = declared_fns
        .iter()
        .filter(|f| !f.is_test)
        .map(|f| f.name.as_str())
        .collect();

    let mut warnings = Vec::new();
    for (path, _, syntax) in parsed {
        let test_fns = collect_test_references(syntax);
        for test_fn in &test_fns {
            let calls_prod = test_fn.call_targets.iter().any(|target| {
                prod_fn_names.contains(target.as_str())
                    || scope.functions.contains(target)
                    || scope.methods.contains(target)
                    || reaches_prod.contains(target)
            }) || test_fn
                .type_qualified_calls
                .iter()
                .any(|type_name| scope.types.contains(type_name));
            if !calls_prod {
                warnings.push(TqWarning {
                    file: path.clone(),
                    line: test_fn.line,
                    function_name: test_fn.name.clone(),
                    kind: TqWarningKind::NoSut,
                    suppressed: false,
                });
            }
        }
    }
    warnings
}
