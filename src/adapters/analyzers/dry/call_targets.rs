use std::collections::HashSet;

use syn::visit::Visit;

use super::split_names::{collect_split, test_scoped_visits, SplitCollector, SplitNames};
use crate::adapters::shared::{macro_params, macro_tokens};

// ── Call target collection ──────────────────────────────────────

/// Collect all function/method call targets from all parsed files,
/// separated into production and test contexts.
/// Trivial: creates visitor and delegates via for_each closure.
pub(crate) fn collect_all_calls(
    parsed: &[(String, String, syn::File)],
    cfg_test_files: &HashSet<String>,
) -> SplitNames {
    // One walk over the macro definitions, two questions about them.
    let bodies = super::macro_reach::collect_macro_bodies(parsed);
    let mut collector = CallTargetCollector {
        macro_reach: super::macro_reach::macro_reach_of(&bodies),
        call_through: super::macro_reach::call_through_macros(&bodies),
        ..Default::default()
    };
    collect_split(parsed, cfg_test_files, &mut collector)
}

/// AST visitor that collects all function/method call targets.
#[derive(Default)]
struct CallTargetCollector {
    names: SplitNames,
    /// What each `macro_rules!` macro's body names. A test that invokes a macro
    /// runs whatever the definition names, and that edge is invisible to a
    /// walker that does not expand macros.
    macro_reach: super::macro_reach::MacroReach,
    /// Macros that call through a metavariable, and which argument positions
    /// they apply — see `macro_reach::call_through_macros`. At an invocation of
    /// one of these, the argument at a called position is the function that
    /// really runs, and it arrives as a bare ident no token walk can recognise
    /// as a call.
    call_through: macro_params::CalledPositions,
}

impl SplitCollector for CallTargetCollector {
    fn names(&mut self) -> &mut SplitNames {
        &mut self.names
    }
}

/// Insert the last path segment and qualified `Type::method` form into the target set.
fn insert_path_segments(target: &mut HashSet<String>, path: &syn::Path) {
    let segments: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    let Some(last) = segments.last() else {
        return;
    };
    target.insert(last.clone());
    if segments.len() >= 2 {
        target.insert(format!("{}::{}", segments[segments.len() - 2], last));
    }
}

impl CallTargetCollector {
    /// The call-target set for the current context (test vs production).
    /// Trivial: delegates to the shared split.
    fn target(&mut self) -> &mut HashSet<String> {
        self.names.target()
    }

    /// A macro invoked from a test runs what its definition names, so those
    /// names are test-reached. Only from test context: the same generosity on
    /// the production side would hide dead code, whereas here it can only
    /// suppress a finding.
    /// Operation: lookup + bulk insert, no own calls.
    fn reach_through_macro(&mut self, node: &syn::Macro) {
        if !self.names.in_test {
            return;
        }
        let reached = node
            .path
            .segments
            .last()
            .and_then(|s| self.macro_reach.get(&s.ident.to_string()))
            .cloned()
            .unwrap_or_default();
        self.names.refs.tests.extend(reached);
    }

    /// At an invocation of a call-through macro, the arguments at the positions
    /// it applies are possible callees — each arrives as a bare name, with
    /// nothing syntactic to mark it as a call. Only those positions: `step!`
    /// applying its first argument says nothing about the second, and taking
    /// every identifier made a plainly dead function look called. When the
    /// target's matcher does not admit positions the coarse form remains, which
    /// suppresses a finding rather than inventing one. The consumer intersects
    /// the result against declared function names, and an ordinary `assert_eq!`
    /// never reaches here at all.
    /// Operation: membership check + bulk insert, no own calls.
    fn arguments_of_a_call_through(&mut self, node: &syn::Macro) {
        let invoked = node.path.segments.last().map(|s| s.ident.to_string());
        let Some(positions) = invoked.and_then(|name| self.call_through.get(&name).cloned()) else {
            return;
        };
        let idents: Vec<String> = macro_params::called_arguments(&node.tokens, &positions)
            .iter()
            .flat_map(macro_tokens::all_idents)
            .collect();
        self.target().extend(idents);
    }

    /// Extract function names referenced by serde field attributes.
    /// Operation: attribute parsing logic, no own calls.
    fn extract_serde_fn_refs(attrs: &[syn::Attribute]) -> Vec<String> {
        let mut refs = Vec::new();
        let push_fn_ref = |refs: &mut Vec<String>, s: String| {
            if let Some(name) = s.rsplit("::").next() {
                refs.push(name.to_string());
            }
            if s.contains("::") {
                refs.push(s);
            }
        };
        attrs
            .iter()
            .filter(|a| a.path().is_ident("serde"))
            .for_each(|attr| {
                let _ = attr.parse_nested_meta(|meta| {
                    let is_fn_key = meta.path.is_ident("deserialize_with")
                        || meta.path.is_ident("serialize_with")
                        || meta.path.is_ident("default");
                    if is_fn_key || meta.path.is_ident("with") {
                        if let Ok(value) = meta.value() {
                            if let Ok(lit) = value.parse::<syn::LitStr>() {
                                let s = lit.value();
                                if is_fn_key {
                                    push_fn_ref(&mut refs, s);
                                } else {
                                    refs.push(format!("{s}::serialize"));
                                    refs.push(format!("{s}::deserialize"));
                                    refs.extend(["serialize".into(), "deserialize".into()]);
                                }
                            }
                        }
                    }
                    Ok(())
                });
            });
        refs
    }

    /// Extract function references from call arguments (e.g., `.for_each(some_fn)`).
    fn record_path_args(
        &mut self,
        args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    ) {
        let target = self.names.target();
        args.iter().for_each(|arg| {
            let expr = match arg {
                syn::Expr::Reference(r) => &*r.expr,
                other => other,
            };
            if let syn::Expr::Path(p) = expr {
                insert_path_segments(target, &p.path);
            }
        });
    }
}

impl<'ast> Visit<'ast> for CallTargetCollector {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*node.func {
            insert_path_segments(self.names.target(), &p.path);
        }
        self.record_path_args(&node.args);
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let name = node.method.to_string();
        self.names.target().insert(name);
        self.record_path_args(&node.args);
        syn::visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        let target = self.names.target();
        node.fields.iter().for_each(|field| {
            if let syn::Expr::Path(p) = &field.expr {
                insert_path_segments(target, &p.path);
            }
        });
        syn::visit::visit_expr_struct(self, node);
    }

    test_scoped_visits!();

    /// Scoped like the generated kinds, plus the function names serde
    /// attributes carry (`deserialize_with = "…"`) that nothing else calls.
    fn visit_field(&mut self, node: &'ast syn::Field) {
        let previous = self.names.enter(&node.attrs);
        let refs = Self::extract_serde_fn_refs(&node.attrs);
        self.target().extend(refs);
        syn::visit::visit_field(self, node);
        self.names.leave(previous);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        // Macro bodies are opaque to syn's visitor; recover embedded exprs so
        // calls inside assert!(), format!(), vec![] — including the `;`-repeat
        // and block-bodied forms — become call-graph edges.
        crate::adapters::shared::macro_tokens::recover_exprs(&node.tokens)
            .iter()
            .for_each(|expr| syn::visit::visit_expr(self, expr));
        // Always also harvest call/construction-position idents from the body.
        // DSL component invocations use struct syntax (`Component { .. }`) that
        // the structured visit parses but does NOT record as a call. Harvesting
        // only idents in call/construction position (not prop keys or locals)
        // keeps the over-collection tight; for reachability it only ever
        // *suppresses* a finding, never raises a false one (a rare colliding
        // name can mask a true positive — the accepted conservative bias).
        let target = self.target();
        crate::adapters::shared::macro_tokens::idents_in_call_position(&node.tokens).for_each(
            |id| {
                target.insert(id);
            },
        );
        self.arguments_of_a_call_through(node);
        self.reach_through_macro(node);
        syn::visit::visit_macro(self, node);
    }

    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        // Only pub/pub(crate) re-exports count as usage of the original function.
        // Private `use` imports are not re-exports; their call targets are already
        // captured via visit_expr_call when the imported name is actually called.
        if matches!(node.vis, syn::Visibility::Inherited) {
            return;
        }
        // A re-export is usage, not a call — it goes to its own set so the two
        // consumers can ask their own question of it. In test context it stays
        // with the test calls: a `pub use` inside a `#[cfg(test)]` module is
        // test-side usage, and folding it into production would report every
        // marker on a test-only helper as spent.
        let target = match self.names.in_test {
            true => &mut self.names.refs.tests,
            false => &mut self.names.reexported,
        };
        // Iterative UseTree walk
        let mut stack: Vec<&syn::UseTree> = vec![&node.tree];
        while let Some(tree) = stack.pop() {
            match tree {
                syn::UseTree::Name(n) => {
                    target.insert(n.ident.to_string());
                }
                syn::UseTree::Rename(r) => {
                    // Record the ORIGINAL name (r.ident), not the alias (r.rename).
                    target.insert(r.ident.to_string());
                }
                syn::UseTree::Path(p) => stack.push(&p.tree),
                syn::UseTree::Group(g) => stack.extend(&g.items),
                syn::UseTree::Glob(_) => {} // Can't enumerate; skip
            }
        }
        // No need to recurse — ItemUse has no child expressions to visit.
    }
}
