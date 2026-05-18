//! Rewrite `var x = …` → `let x = …`.
//!
//! Operates on source. Tree-sitter-javascript parses `var` declarations into
//! a `variable_declaration` node (vs. `lexical_declaration` for `let`/`const`),
//! and the keyword token is the first anonymous child of that node. We replace
//! exactly its byte range with `let `.

use tree_sitter::{Parser, TreeCursor};

pub fn var_to_let(src: &str) -> String {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_javascript::language())
        .is_err()
    {
        return src.to_string();
    }
    let tree = match parser.parse(src, None) {
        Some(t) => t,
        None => return src.to_string(),
    };

    let mut edits: Vec<(usize, usize)> = Vec::new();
    let mut cursor = tree.walk();
    collect(&mut cursor, src, &mut edits);

    apply_replace(src, edits, "let")
}

fn collect(cursor: &mut TreeCursor, src: &str, edits: &mut Vec<(usize, usize)>) {
    let node = cursor.node();
    if node.kind() == "variable_declaration" {
        // First child is the `var` keyword (anonymous token).
        if let Some(first) = node.child(0) {
            let text = first.utf8_text(src.as_bytes()).unwrap_or("");
            if text == "var" {
                edits.push((first.start_byte(), first.end_byte()));
            }
        }
    }

    if cursor.goto_first_child() {
        loop {
            collect(cursor, src, edits);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

fn apply_replace(src: &str, mut edits: Vec<(usize, usize)>, replacement: &str) -> String {
    edits.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(src.len());
    let mut cursor = 0usize;
    for (start, end) in edits {
        if start < cursor {
            continue;
        }
        out.push_str(&src[cursor..start]);
        out.push_str(replacement);
        cursor = end;
    }
    out.push_str(&src[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_var_to_let_at_top_level() {
        assert_eq!(var_to_let("var x = 1;"), "let x = 1;");
    }

    #[test]
    fn rewrites_var_to_let_in_block() {
        let got = var_to_let("function f() { var y = 2; return y; }");
        assert!(got.contains("let y = 2"));
        assert!(!got.contains("var"));
    }

    #[test]
    fn leaves_const_and_let_alone() {
        assert_eq!(var_to_let("const x = 1;"), "const x = 1;");
        assert_eq!(var_to_let("let y = 2;"), "let y = 2;");
    }

    #[test]
    fn leaves_var_in_string_alone() {
        let src = "let s = \"var keyword in string\";";
        assert_eq!(var_to_let(src), src);
    }

    #[test]
    fn handles_multi_declarator() {
        assert_eq!(var_to_let("var a, b = 1, c;"), "let a, b = 1, c;");
    }
}
