use super::*;

#[test]
fn test_count_production_lines_simple() {
    let source = "fn main() {\n    println!(\"hello\");\n}\n";
    assert_eq!(count_production_lines(source), 3);
}

#[test]
fn test_count_production_lines_with_test_module() {
    let source = "fn main() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn test_it() {}\n}\n";
    // Only "fn main() {}" is production code (1 line of code, blank lines skipped)
    assert_eq!(count_production_lines(source), 1);
}

#[test]
fn test_count_production_lines_skips_comments() {
    let source = "// This is a comment\nfn foo() {}\n// Another comment\nfn bar() {}\n";
    assert_eq!(count_production_lines(source), 2);
}

#[test]
fn test_count_production_lines_skips_blanks() {
    let source = "\n\nfn foo() {}\n\n\nfn bar() {}\n\n";
    assert_eq!(count_production_lines(source), 2);
}

#[test]
fn test_count_production_lines_stops_on_single_line_cfg_test() {
    // Single-line form: `#[cfg(test)] mod tests { ... }` on one line.
    // Previously this would not match `trimmed == "#[cfg(test)]"` and
    // the test body would be counted as production.
    let source = "fn main() {}\n#[cfg(test)] mod tests { fn t() {} fn u() {} }\n";
    assert_eq!(
        count_production_lines(source),
        1,
        "single-line `#[cfg(test)] mod tests` must terminate counting"
    );
}

#[test]
fn test_count_production_lines_stops_on_cfg_test_with_trailing_whitespace() {
    // `#[cfg(test)]    ` (trailing whitespace) — exact-equality check
    // used to skip past this line.
    let source = "fn main() {}\n#[cfg(test)]   \nmod tests { fn t() {} }\n";
    assert_eq!(count_production_lines(source), 1);
}

#[test]
fn test_count_production_lines_skips_block_comment() {
    // Multi-line `/* … */` block: opening line, body lines, and
    // closing line are all non-production.
    let source = "\
/*
 * A multi-line block comment.
 * Spans several lines.
 */
fn foo() {}
";
    assert_eq!(
        count_production_lines(source),
        1,
        "only `fn foo() {{}}` is production"
    );
}

#[test]
fn test_count_production_lines_skips_single_line_block_comment() {
    let source = "/* header */\nfn foo() {}\nfn bar() {}\n";
    assert_eq!(count_production_lines(source), 2);
}

#[test]
fn test_count_production_lines_counts_deref_starting_lines() {
    // Lines starting with `*` outside a block comment are valid code
    // (deref / assign-through pointer) and must count as production.
    let source = "\
fn write(p: &mut i32) {
    *p = 42;
    *p += 1;
}
";
    // Body lines: `fn` signature, 2 deref assignments, closing `}` — 4.
    assert_eq!(count_production_lines(source), 4);
}

#[test]
fn test_count_production_lines_counts_code_before_block_comment() {
    // `let x = 1; /* note */` has code before the comment and must count.
    let source = "fn foo() {\n    let x = 1; /* note */\n}\n";
    assert_eq!(count_production_lines(source), 3);
}

#[test]
fn test_count_production_lines_counts_code_after_inline_block_comment() {
    // `/* note */ let x = 1;` has code AFTER the inline comment.
    // A leading-only heuristic would wrongly skip this.
    let source = "fn foo() {\n    /* note */ let x = 1;\n}\n";
    assert_eq!(count_production_lines(source), 3);
}

#[test]
fn test_count_production_lines_skips_pure_inline_block_comment() {
    let source = "fn foo() {\n    /* note */\n}\n";
    assert_eq!(count_production_lines(source), 2);
}

#[test]
fn test_count_production_lines_handles_nested_block_comments() {
    // Rust supports nested block comments: the inner `*/` must NOT
    // close the outer, so "still outer" stays inside the comment.
    let source = "\
/* outer
   /* inner */
   still outer */
fn foo() {}
";
    assert_eq!(
        count_production_lines(source),
        1,
        "nested block comments must track depth, not a boolean flag"
    );
}

#[test]
fn test_count_production_lines_nested_block_closes_properly() {
    // Confirm depth unwinds correctly: after both `*/` the scanner
    // is back at depth 0 and recognises `fn bar() {}` on the same
    // line as real code.
    let source = "/* a /* b */ c */ fn bar() {}\n";
    assert_eq!(count_production_lines(source), 1);
}

#[test]
fn test_count_production_lines_empty() {
    assert_eq!(count_production_lines(""), 0);
}
