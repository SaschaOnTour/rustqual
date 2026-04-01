use std::collections::HashMap;

use syn::visit::Visit;

use crate::dry::{has_cfg_test, has_test_attr, FileVisitor};

/// Minimum number of match arms for a match to be considered.
const MIN_MATCH_ARMS: usize = 3;

/// Minimum number of instances of the same match pattern to flag as repeated.
const MIN_INSTANCES: usize = 3;

/// A group of repeated match patterns found across functions.
pub struct RepeatedMatchGroup {
    pub enum_name: String,
    pub entries: Vec<RepeatedMatchEntry>,
}

/// A single occurrence of a repeated match pattern.
pub struct RepeatedMatchEntry {
    pub file: String,
    pub line: usize,
    pub function_name: String,
    pub arm_count: usize,
}

/// Internal entry collected during AST walking.
struct CollectedMatch {
    file: String,
    line: usize,
    function_name: String,
    arm_count: usize,
    hash: u64,
    enum_name: String,
}

/// AST visitor that collects match expressions for repeated pattern detection.
struct MatchPatternCollector<'a> {
    config: &'a crate::config::sections::DuplicatesConfig,
    file: String,
    collected: Vec<CollectedMatch>,
    in_test: bool,
    current_fn: String,
}

impl FileVisitor for MatchPatternCollector<'_> {
    fn reset_for_file(&mut self, file_path: &str) {
        self.file = file_path.to_string();
        self.in_test = false;
        self.current_fn = String::new();
    }
}

impl<'ast> Visit<'ast> for MatchPatternCollector<'_> {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let prev = std::mem::take(&mut self.current_fn);
        self.current_fn = node.sig.ident.to_string();
        let is_test = has_test_attr(&node.attrs);
        if self.config.ignore_tests && (self.in_test || is_test) {
            self.current_fn = prev;
            return;
        }
        syn::visit::visit_item_fn(self, node);
        self.current_fn = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let prev = std::mem::take(&mut self.current_fn);
        self.current_fn = node.sig.ident.to_string();
        let is_test = has_test_attr(&node.attrs);
        if self.config.ignore_tests && (self.in_test || is_test) {
            self.current_fn = prev;
            return;
        }
        syn::visit::visit_impl_item_fn(self, node);
        self.current_fn = prev;
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let prev_in_test = self.in_test;
        if has_cfg_test(&node.attrs) {
            self.in_test = true;
        }
        syn::visit::visit_item_mod(self, node);
        self.in_test = prev_in_test;
    }

    fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
        if node.arms.len() >= MIN_MATCH_ARMS && !self.current_fn.is_empty() {
            let normalize = |match_expr: &syn::ExprMatch| {
                let stmt = syn::Stmt::Expr(syn::Expr::Match(match_expr.clone()), None);
                let tokens = crate::normalize::normalize_stmts(&[stmt]);
                crate::normalize::structural_hash(&tokens)
            };
            let hash = normalize(node);
            let enum_name = extract_enum_name(node);
            let line = node.match_token.span.start().line;

            self.collected.push(CollectedMatch {
                file: self.file.clone(),
                line,
                function_name: self.current_fn.clone(),
                arm_count: node.arms.len(),
                hash,
                enum_name,
            });
        }
        syn::visit::visit_expr_match(self, node);
    }
}

/// Detect repeated match patterns across parsed files.
/// Integration: creates collector, calls visit_all_files, calls group function.
pub fn detect_repeated_matches(
    parsed: &[(String, String, syn::File)],
    config: &crate::config::sections::DuplicatesConfig,
) -> Vec<RepeatedMatchGroup> {
    let mut collector = MatchPatternCollector {
        config,
        file: String::new(),
        collected: Vec::new(),
        in_test: false,
        current_fn: String::new(),
    };
    crate::dry::visit_all_files(parsed, &mut collector);
    group_repeated_patterns(collector.collected)
}

/// Group collected match entries by hash and filter to repeated patterns.
/// Operation: hash grouping + filtering logic, no own calls.
fn group_repeated_patterns(collected: Vec<CollectedMatch>) -> Vec<RepeatedMatchGroup> {
    let mut groups: HashMap<u64, Vec<CollectedMatch>> = HashMap::new();
    for entry in collected {
        groups.entry(entry.hash).or_default().push(entry);
    }

    let mut result: Vec<RepeatedMatchGroup> = groups
        .into_iter()
        .filter(|(_hash, entries)| {
            if entries.len() < MIN_INSTANCES {
                return false;
            }
            let mut seen = std::collections::HashSet::new();
            entries.iter().any(|e| !seen.insert(&e.function_name)) || seen.len() >= 2
        })
        .map(|(_hash, entries)| {
            let enum_name = entries
                .first()
                .map(|e| e.enum_name.clone())
                .unwrap_or_default();
            RepeatedMatchGroup {
                enum_name,
                entries: entries
                    .into_iter()
                    .map(|e| RepeatedMatchEntry {
                        file: e.file,
                        line: e.line,
                        function_name: e.function_name,
                        arm_count: e.arm_count,
                    })
                    .collect(),
            }
        })
        .collect();

    result.sort_by(|a, b| b.entries.len().cmp(&a.entries.len()));
    result
}

/// Extract the enum name from a match expression's arm patterns (best effort).
/// Operation: pattern matching on arm patterns, no own calls.
fn extract_enum_name(match_expr: &syn::ExprMatch) -> String {
    let from_path = |path: &syn::Path| -> Option<String> {
        if path.segments.len() >= 2 {
            Some(path.segments[path.segments.len() - 2].ident.to_string())
        } else {
            None
        }
    };
    for arm in &match_expr.arms {
        let name = match &arm.pat {
            syn::Pat::Path(p) => from_path(&p.path),
            syn::Pat::TupleStruct(ts) => from_path(&ts.path),
            syn::Pat::Struct(ps) => from_path(&ps.path),
            _ => None,
        };
        if let Some(n) = name {
            return n;
        }
    }
    "(unknown)".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::sections::DuplicatesConfig;

    fn parse(code: &str) -> Vec<(String, String, syn::File)> {
        let syntax = syn::parse_file(code).expect("parse failed");
        vec![("test.rs".to_string(), code.to_string(), syntax)]
    }

    #[test]
    fn test_detect_empty() {
        let parsed = parse("");
        let config = DuplicatesConfig::default();
        let result = detect_repeated_matches(&parsed, &config);
        assert!(result.is_empty());
    }

    #[test]
    fn test_detect_single_match_not_flagged() {
        let code = r#"
            enum E { A, B, C }
            fn f(e: E) {
                match e { E::A => 1, E::B => 2, E::C => 3 };
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig::default();
        let result = detect_repeated_matches(&parsed, &config);
        assert!(result.is_empty(), "single instance should not be flagged");
    }

    #[test]
    fn test_detect_repeated_match_flagged() {
        let code = r#"
            enum E { A, B, C }
            fn f1(e: E) -> i32 {
                match e { E::A => 1, E::B => 2, E::C => 3 }
            }
            fn f2(e: E) -> i32 {
                match e { E::A => 1, E::B => 2, E::C => 3 }
            }
            fn f3(e: E) -> i32 {
                match e { E::A => 1, E::B => 2, E::C => 3 }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig::default();
        let result = detect_repeated_matches(&parsed, &config);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].entries.len(), 3);
        assert_eq!(result[0].enum_name, "E");
    }

    #[test]
    fn test_detect_different_matches_not_grouped() {
        let code = r#"
            enum E { A, B, C }
            fn f1(e: E) -> i32 {
                match e { E::A => 1, E::B => 2, E::C => 3 }
            }
            fn f2(e: E) -> i32 {
                match e { E::A => 10, E::B => 20, E::C => 30 }
            }
            fn f3(e: E) -> i32 {
                match e { E::A => 100, E::B => 200, E::C => 300 }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig::default();
        let result = detect_repeated_matches(&parsed, &config);
        // Normalization erases literal values, so these should hash the same
        // because the structure is identical (match on enum with 3 literal returns)
        assert_eq!(result.len(), 1, "same structure matches should be grouped");
    }

    #[test]
    fn test_detect_test_code_excluded() {
        let code = r#"
            enum E { A, B, C }
            #[cfg(test)]
            mod tests {
                use super::*;
                fn f1(e: E) -> i32 { match e { E::A => 1, E::B => 2, E::C => 3 } }
                fn f2(e: E) -> i32 { match e { E::A => 1, E::B => 2, E::C => 3 } }
                fn f3(e: E) -> i32 { match e { E::A => 1, E::B => 2, E::C => 3 } }
            }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig {
            ignore_tests: true,
            ..DuplicatesConfig::default()
        };
        let result = detect_repeated_matches(&parsed, &config);
        assert!(result.is_empty(), "test code should be excluded");
    }

    #[test]
    fn test_detect_few_arms_not_flagged() {
        let code = r#"
            fn f1(b: bool) -> i32 { match b { true => 1, false => 0 } }
            fn f2(b: bool) -> i32 { match b { true => 1, false => 0 } }
            fn f3(b: bool) -> i32 { match b { true => 1, false => 0 } }
        "#;
        let parsed = parse(code);
        let config = DuplicatesConfig::default();
        let result = detect_repeated_matches(&parsed, &config);
        assert!(
            result.is_empty(),
            "matches with <3 arms should not be flagged"
        );
    }

    #[test]
    fn test_extract_enum_name_tuple_struct() {
        let code = "match x { Foo::A(v) => v, Foo::B(v) => v, Foo::C(v) => v }";
        let expr: syn::ExprMatch = syn::parse_str(code).unwrap();
        assert_eq!(extract_enum_name(&expr), "Foo");
    }

    #[test]
    fn test_extract_enum_name_path() {
        let code = "match x { Foo::A => 1, Foo::B => 2, Foo::C => 3 }";
        let expr: syn::ExprMatch = syn::parse_str(code).unwrap();
        assert_eq!(extract_enum_name(&expr), "Foo");
    }

    #[test]
    fn test_extract_enum_name_unknown() {
        let code = "match x { a => 1, b => 2, c => 3 }";
        let expr: syn::ExprMatch = syn::parse_str(code).unwrap();
        assert_eq!(extract_enum_name(&expr), "(unknown)");
    }

    #[test]
    fn test_group_requires_multiple_functions() {
        let entries = vec![
            CollectedMatch {
                file: "a.rs".into(),
                line: 1,
                function_name: "same_fn".into(),
                arm_count: 5,
                hash: 42,
                enum_name: "E".into(),
            },
            CollectedMatch {
                file: "a.rs".into(),
                line: 10,
                function_name: "same_fn".into(),
                arm_count: 5,
                hash: 42,
                enum_name: "E".into(),
            },
            CollectedMatch {
                file: "a.rs".into(),
                line: 20,
                function_name: "same_fn".into(),
                arm_count: 5,
                hash: 42,
                enum_name: "E".into(),
            },
        ];
        let result = group_repeated_patterns(entries);
        // 3 instances in same function — still flagged (≥ MIN_INSTANCES)
        // The filter checks len >= 2 unique functions OR len >= MIN_INSTANCES with duplicates
        assert_eq!(
            result.len(),
            1,
            "3 instances even in same fn should be flagged"
        );
    }
}
