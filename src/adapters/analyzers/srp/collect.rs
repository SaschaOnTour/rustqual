//! AST collectors for struct-SRP analysis: gather struct definitions
//! ([`StructCollector`]) and their impl-method field/call footprints
//! ([`ImplMethodCollector`] + [`MethodBodyVisitor`]). Both qualify a type's
//! pooling identity with [`owner_key`] (file + inline-module path) so
//! same-named types in different files / modules never share a method bucket.

use std::collections::HashSet;

use syn::visit::Visit;

use crate::adapters::shared::file_visitor::FileVisitor;

use super::{MethodFieldData, StructInfo};

/// Join a type's file and module-qualified segments into its pooling identity:
/// `file::mod1::mod2::Name` (no module → `file::Name`). This is what keeps
/// same-named structs in different files / inline modules from sharing a method
/// bucket. Out-of-line `mod name;` bodies live in their own file, so the file
/// segment already separates them.
/// Operation: string join, no own calls.
fn join_owner_key(file: &str, segments: &[String]) -> String {
    let mut all: Vec<&str> = vec![file];
    all.extend(segments.iter().map(String::as_str));
    all.join("::")
}

/// A struct's owner segments: its enclosing inline-module stack plus its name.
/// Operation: clone + push, no own calls.
fn struct_owner_segments(module_stack: &[String], name: &str) -> Vec<String> {
    let mut segments = module_stack.to_vec();
    segments.push(name.to_string());
    segments
}

/// Resolve an impl's *relative* self-type path (`Foo`, `inner::Foo`,
/// `super::Foo`, `self::Foo`) against the impl's inline-module stack into the
/// SAME module-qualified segments `struct_owner_segments` produced for the type
/// — otherwise a qualified `impl inner::Foo` keys differently from its struct
/// and their methods stop pooling (false-negative god-structs). A leading
/// `self::` is dropped and each `super::` climbs one inline module; a bare or
/// `inner::`-relative path extends the current stack.
///
/// Absolute paths (`crate::a::Foo`, `::ext::Foo`) are deliberately NOT resolved:
/// they name the *crate* module hierarchy, which the `file + inline-stack` key
/// does not model (a file-backed module's crate path is the file path, not a
/// stack entry), and deriving it would need the fragile file-path→module
/// mapping rustqual avoids. The leading `crate`/`::` segments are kept as-is, so
/// the key simply never matches a struct and the impl does not pool — a
/// safe-direction under-report (never a false positive), like a cross-file
/// split impl. Pinned by `impl_with_crate_absolute_path_is_accepted_under_report`.
/// Operation: path-prefix match + stack arithmetic, no own calls.
fn impl_owner_segments(module_stack: &[String], self_ty_path: &[String]) -> Vec<String> {
    let mut stack = module_stack.to_vec();
    let mut rest = self_ty_path;
    match rest.first().map(String::as_str) {
        Some("self") => rest = &rest[1..],
        Some("super") => {
            while rest.first().map(String::as_str) == Some("super") {
                stack.pop();
                rest = &rest[1..];
            }
        }
        _ => {}
    }
    stack.extend(rest.iter().cloned());
    stack
}

/// AST visitor that collects struct definitions with their named fields.
pub(crate) struct StructCollector<'a> {
    pub file: String,
    /// Enclosing inline-`mod` names (innermost last), to qualify the owner key.
    pub module_stack: Vec<String>,
    pub structs: &'a mut Vec<StructInfo>,
}

impl FileVisitor for StructCollector<'_> {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
        self.module_stack.clear();
    }
}

impl<'ast, 'a> Visit<'ast> for StructCollector<'a> {
    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        let fields: Vec<String> = node
            .fields
            .iter()
            .filter_map(|f| f.ident.as_ref().map(|id| id.to_string()))
            .collect();
        // Only track named-field structs (skip tuple structs and unit structs)
        if !fields.is_empty() {
            let name = node.ident.to_string();
            let segments = struct_owner_segments(&self.module_stack, &name);
            self.structs.push(StructInfo {
                owner_key: join_owner_key(&self.file, &segments),
                name,
                file: self.file.clone(),
                line: node.ident.span().start().line,
                fields,
            });
        }
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        // Only inline `mod name { … }` qualifies the path; a bare `mod name;`
        // has its body in a separate file the outer walk visits on its own.
        let pushed = node.content.is_some();
        if pushed {
            self.module_stack.push(node.ident.to_string());
        }
        syn::visit::visit_item_mod(self, node);
        if pushed {
            self.module_stack.pop();
        }
    }
}

/// Trait names whose impl methods are mechanical (touch all fields to format,
/// compare, convert, hash, or (de)serialize). Their cohesion is meaningless and
/// would mask real god-structs, so they are excluded entirely — neither
/// responsibility nodes nor bridges.
const MECHANICAL_TRAITS: &[&str] = &[
    "Display",
    "Debug",
    "Default",
    "Clone",
    "Copy",
    "Hash",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "From",
    "Into",
    "TryFrom",
    "TryInto",
    "AsRef",
    "AsMut",
    "Borrow",
    "BorrowMut",
    "Serialize",
    "Deserialize",
    "Drop",
    "Deref",
    "DerefMut",
];

/// Whether a trait path names a mechanical trait (matched on its last segment).
/// Operation: last-segment lookup against the blacklist.
fn is_mechanical_trait(path: &syn::Path) -> bool {
    path.segments
        .last()
        .map(|s| s.ident.to_string())
        .is_some_and(|name| MECHANICAL_TRAITS.contains(&name.as_str()))
}

/// AST visitor that collects method field accesses and call targets from impl
/// blocks. Inherent methods become cohesion **nodes** (`methods`); non-mechanical
/// trait methods (e.g. `Visit`) become **bridges** — their field footprint
/// unions the inherent methods they tie together without counting as a separate
/// responsibility (so a visitor's helpers cohere instead of fragmenting).
pub(crate) struct ImplMethodCollector<'a> {
    pub file: String,
    /// Enclosing inline-`mod` names (innermost last); must match the struct's
    /// stack so an impl pools with the right same-named type.
    pub module_stack: Vec<String>,
    pub methods: &'a mut Vec<MethodFieldData>,
    pub bridges: &'a mut Vec<MethodFieldData>,
}

impl FileVisitor for ImplMethodCollector<'_> {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
        self.module_stack.clear();
    }
}

impl<'ast, 'a> Visit<'ast> for ImplMethodCollector<'a> {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        // Only inline modules qualify the path (mirrors StructCollector); an
        // out-of-line `mod name;` body is a separate file visited on its own.
        let pushed = node.content.is_some();
        if pushed {
            self.module_stack.push(node.ident.to_string());
        }
        syn::visit::visit_item_mod(self, node);
        if pushed {
            self.module_stack.pop();
        }
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        // Full self-type path segments (`inner::Foo` → ["inner", "Foo"]), so a
        // qualified impl resolves to the same owner key as its struct; generic
        // args are ignored. `parent_type` keeps the bare last segment for display.
        let self_ty_path: Vec<String> = if let syn::Type::Path(tp) = &*node.self_ty {
            tp.path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect()
        } else {
            Vec::new()
        };
        let Some(type_name) = self_ty_path.last().cloned() else {
            syn::visit::visit_item_impl(self, node);
            return;
        };
        // Mechanical trait impls (Display, serde, …) are excluded entirely.
        if node
            .trait_
            .as_ref()
            .is_some_and(|(_, p, _)| is_mechanical_trait(p))
        {
            syn::visit::visit_item_impl(self, node);
            return;
        }
        // Non-mechanical trait impls contribute bridges; inherent impls, nodes.
        let is_bridge = node.trait_.is_some();
        let segments = impl_owner_segments(&self.module_stack, &self_ty_path);
        let key = join_owner_key(&self.file, &segments);

        for item in &node.items {
            if let syn::ImplItem::Fn(method) = item {
                if let Some(data) = method_field_data(method, &type_name, &key) {
                    if is_bridge {
                        self.bridges.push(data);
                    } else {
                        self.methods.push(data);
                    }
                }
            }
        }
        // Don't call default visit — we already handled methods manually
    }
}

/// Build the field/call footprint for one impl method, or `None` when it is
/// neither an instance method nor a constructor (a plain associated fn — not a
/// cohesion node). Operation: receiver/return classification + body walk.
fn method_field_data(
    method: &syn::ImplItemFn,
    parent_type: &str,
    owner_key: &str,
) -> Option<MethodFieldData> {
    let is_instance = method.sig.receiver().is_some();
    let is_constructor = !is_instance && returns_self(&method.sig.output);
    if !is_instance && !is_constructor {
        return None;
    }
    let mut body_visitor = MethodBodyVisitor {
        field_accesses: HashSet::new(),
        call_targets: HashSet::new(),
        self_method_calls: HashSet::new(),
    };
    body_visitor.visit_block(&method.block);
    Some(MethodFieldData {
        method_name: method.sig.ident.to_string(),
        parent_type: parent_type.to_string(),
        owner_key: owner_key.to_string(),
        field_accesses: body_visitor.field_accesses,
        call_targets: body_visitor.call_targets,
        self_method_calls: body_visitor.self_method_calls,
        is_constructor,
    })
}

/// Visitor that walks a method body to find self.field accesses and call targets.
pub(crate) struct MethodBodyVisitor {
    pub field_accesses: HashSet<String>,
    pub call_targets: HashSet<String>,
    pub self_method_calls: HashSet<String>,
}

impl<'ast> Visit<'ast> for MethodBodyVisitor {
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        match expr {
            // Detect self.field_name
            syn::Expr::Field(ef) => {
                if is_self_expr(&ef.base) {
                    if let syn::Member::Named(ident) = &ef.member {
                        self.field_accesses.insert(ident.to_string());
                    }
                }
                syn::visit::visit_expr(self, expr);
            }
            // Detect function calls for fan-out: Type::method() or function()
            syn::Expr::Call(ec) => {
                if let syn::Expr::Path(ep) = &*ec.func {
                    let path_str = ep
                        .path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect::<Vec<_>>()
                        .join("::");
                    self.call_targets.insert(path_str);
                }
                syn::visit::visit_expr(self, expr);
            }
            // Detect method calls: obj.method()
            syn::Expr::MethodCall(mc) => {
                if is_self_expr(&mc.receiver) {
                    self.self_method_calls.insert(mc.method.to_string());
                } else {
                    self.call_targets.insert(mc.method.to_string());
                }
                syn::visit::visit_expr(self, expr);
            }
            _ => {
                syn::visit::visit_expr(self, expr);
            }
        }
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        // Recover the macro body's expressions and feed them through our own
        // `visit_expr` override, so `self.field` accesses and `self.method()`
        // calls inside `debug_assert!(...)`, `assert_eq!(...)`, `format!(...)`
        // — including the `;`-repeat and block-bodied forms — contribute to
        // LCOM4 just like non-macro code. We must call `self.visit_expr(e)`
        // (the override) rather than `syn::visit::visit_expr(self, e)` (the
        // dispatcher) — the dispatcher skips our outer match arm and walks
        // straight into sub-expressions, so the method-call ident itself
        // would never be recorded.
        for expr in crate::adapters::shared::macro_tokens::recover_exprs(&node.tokens) {
            self.visit_expr(&expr);
        }
        syn::visit::visit_macro(self, node);
    }
}

/// Check if a function's return type contains Self (constructor pattern).
/// Handles `-> Self`, `-> Result<Self, E>`, `-> Option<Self>`, etc.
/// Operation: pattern matching with closures for IOSP.
pub(crate) fn returns_self(output: &syn::ReturnType) -> bool {
    let syn::ReturnType::Type(_, ty) = output else {
        return false;
    };
    let syn::Type::Path(tp) = &**ty else {
        return false;
    };
    // Direct Self
    if tp.path.segments.last().is_some_and(|s| s.ident == "Self") {
        return true;
    }
    // Self inside one level of generics: Result<Self, E>, Option<Self>, etc.
    tp.path.segments.iter().any(|seg| {
        matches!(&seg.arguments, syn::PathArguments::AngleBracketed(args)
            if args.args.iter().any(|arg| matches!(arg,
                syn::GenericArgument::Type(syn::Type::Path(inner))
                if inner.path.segments.last().is_some_and(|s| s.ident == "Self")
            ))
        )
    })
}

/// Check if an expression is `self`.
/// Operation: pattern matching.
pub(crate) fn is_self_expr(expr: &syn::Expr) -> bool {
    if let syn::Expr::Path(ep) = expr {
        ep.path.is_ident("self")
    } else {
        false
    }
}
