//! Shared test fixture for `infer_type` and friends.
//!
//! `TypeInferFixture` owns the borrowed inputs the inference engine
//! needs (workspace index, alias map, local symbols, crate roots,
//! bindings, file path, self-type). Tests mutate its public fields
//! directly — no `&mut self` helper methods, which keeps the struct
//! SRP-clean and NMS-compliant.

use crate::adapters::analyzers::architecture::call_parity_rule::local_symbols::FileScope;
use crate::adapters::analyzers::architecture::call_parity_rule::type_infer::{
    BindingLookup, FlatBindings, InferContext, WorkspaceTypeIndex,
};
use crate::adapters::shared::use_tree::ScopedAliasMap;
use std::collections::{HashMap, HashSet};

/// Parse a Rust pattern source string into `syn::Pat`. Tries `let …`
/// first (supports `x: T` annotations) and falls back to a `match` arm
/// (supports refutable patterns like `None`, `a | b`).
pub(super) fn parse_pat(src: &str) -> syn::Pat {
    if let Ok(pat) = parse_pat_as_let(src) {
        return pat;
    }
    parse_pat_as_match_arm(src)
}

fn parse_pat_as_let(src: &str) -> Result<syn::Pat, ()> {
    let wrapped = format!("fn __t() {{ let {} = _todo; }}", src);
    let file: syn::File = syn::parse_str(&wrapped).map_err(|_| ())?;
    let syn::Item::Fn(item_fn) = &file.items[0] else {
        return Err(());
    };
    let syn::Stmt::Local(local) = &item_fn.block.stmts[0] else {
        return Err(());
    };
    Ok(local.pat.clone())
}

fn parse_pat_as_match_arm(src: &str) -> syn::Pat {
    let wrapped = format!("fn __t() {{ match _x {{ {} => () }} }}", src);
    let file: syn::File = syn::parse_str(&wrapped).expect("parse wrapper");
    let syn::Item::Fn(item_fn) = &file.items[0] else {
        panic!("expected fn")
    };
    let syn::Stmt::Expr(syn::Expr::Match(m), _) = &item_fn.block.stmts[0] else {
        panic!("expected match expr")
    };
    m.arms[0].pat.clone()
}

pub(super) struct TypeInferFixture {
    pub index: WorkspaceTypeIndex,
    pub alias_map: HashMap<String, Vec<String>>,
    pub aliases_per_scope: ScopedAliasMap,
    pub local_symbols: HashSet<String>,
    pub local_decl_scopes: HashMap<String, Vec<Vec<String>>>,
    pub crate_roots: HashSet<String>,
    pub bindings: FlatBindings,
    pub file_path: String,
    pub self_type: Option<Vec<String>>,
}

impl TypeInferFixture {
    pub fn new() -> Self {
        Self {
            index: WorkspaceTypeIndex::new(),
            alias_map: HashMap::new(),
            aliases_per_scope: ScopedAliasMap::new(),
            local_symbols: HashSet::new(),
            local_decl_scopes: HashMap::new(),
            crate_roots: HashSet::new(),
            bindings: FlatBindings::new(),
            file_path: "src/app/test.rs".to_string(),
            self_type: None,
        }
    }

    pub fn file_scope(&self) -> FileScope<'_> {
        FileScope {
            path: &self.file_path,
            alias_map: &self.alias_map,
            aliases_per_scope: &self.aliases_per_scope,
            local_symbols: &self.local_symbols,
            local_decl_scopes: &self.local_decl_scopes,
            crate_root_modules: &self.crate_roots,
        }
    }

    pub fn ctx<'a>(&'a self, file_scope: &'a FileScope<'a>) -> InferContext<'a> {
        InferContext {
            file: file_scope,
            mod_stack: &[],
            workspace: &self.index,
            bindings: &self.bindings as &dyn BindingLookup,
            self_type: self.self_type.clone(),
            workspace_files: None,
        }
    }
}
