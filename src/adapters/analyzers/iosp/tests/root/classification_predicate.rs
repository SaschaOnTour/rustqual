use super::*;

type Pred = fn(&Classification) -> bool;
const NOT_VIOLATION: Pred = |c| !matches!(c, Classification::Violation { .. });

// Closures and iterator chains don't count as logic/own-calls in lenient
// (default) mode, so a fn whose only logic/calls live inside them is not a
// Violation; `strict_closures` / `strict_iterator_chains` flip that. A genuine
// logic+own-call mix is always a Violation; stdlib path ctors (String::new)
// never make an integration.
// (label, code, fn_name, strict_closures, strict_iterators, predicate)
const PREDICATE_CASES: &[(&str, &str, &str, bool, bool, Pred)] = &[
    (
        "logic+own-call mix is a violation",
        r#"
            fn helper(x: i32) { if x > 0 { violator(x); } }
            fn violator(x: i32) {
                let _y = x;
                if _y > 0 {
                    helper(_y);
                }
            }
            "#,
        "violator",
        false,
        false,
        |c| matches!(c, Classification::Violation { .. }),
    ),
    (
        "lenient: logic inside a closure is ignored",
        r#"
            fn f() {
                let v = vec![1, 2, 3];
                let _: Vec<_> = v.into_iter().collect();
                let _ = (|| { if true { 1 } else { 2 } })();
            }
            "#,
        "f",
        false,
        false,
        NOT_VIOLATION,
    ),
    (
        "strict closures: logic inside a closure counts",
        r#"
            fn f() {
                let v = vec![1, 2, 3];
                let _ = (|| { if true { 1 } else { 2 } })();
                let _ = v.len();
            }
            "#,
        "f",
        true,
        false,
        |c| {
            matches!(
                c,
                Classification::Operation | Classification::Violation { .. }
            )
        },
    ),
    (
        "lenient: own call inside a closure is ignored",
        r#"
            fn helper() {}
            fn f() {
                let c = || { helper(); };
                c();
                let _ = 1;
            }
            "#,
        "f",
        false,
        false,
        NOT_VIOLATION,
    ),
    (
        "strict closures: own call inside a closure counts",
        r#"
            fn helper() {}
            fn f() {
                let c = || { helper(); };
                c();
                let _ = 1;
            }
            "#,
        "f",
        true,
        false,
        |c| {
            matches!(
                c,
                Classification::Integration | Classification::Violation { .. }
            )
        },
    ),
    (
        "lenient: iterator methods are not own calls",
        r#"
            fn f() -> Vec<i32> {
                let v = vec![1, 2, 3];
                v.iter().map(|x| x + 1).collect()
            }
            "#,
        "f",
        false,
        false,
        NOT_VIOLATION,
    ),
    (
        "strict iterators: in-scope iterator methods count",
        r#"
            struct Foo;
            impl Foo {
                fn map(&self) {}
            }
            fn f() -> Vec<i32> {
                let v = vec![1, 2, 3];
                let x = v.iter().map(|x| x + 1).collect::<Vec<_>>();
                x
            }
            "#,
        "f",
        false,
        true,
        |c| {
            matches!(
                c,
                Classification::Integration | Classification::Violation { .. }
            )
        },
    ),
    (
        "stdlib path ctor is not an own call",
        r#"
            fn f() {
                let _a = String::new();
                let _b = String::new();
            }
            "#,
        "f",
        false,
        false,
        |c| !matches!(c, Classification::Integration),
    ),
];

#[test]
fn classification_predicate_lenient_vs_strict() {
    for (label, code, fn_name, strict_closures, strict_iterators, predicate) in PREDICATE_CASES {
        let mut config = Config::default();
        config.strict_closures = *strict_closures;
        config.strict_iterator_chains = *strict_iterators;
        let classification = classify_cfg(code, fn_name, &config);
        assert!(
            predicate(&classification),
            "case {label}: unexpected {classification:?}"
        );
    }
}
