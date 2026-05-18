//! Drop trailing commas inside array literals, object literals, argument
//! lists, parameter lists, and named import lists. The base- and head-side
//! sources should be byte-identical w.r.t. these decorative commas after
//! normalization, so the AST comparator never sees them.

use tree_sitter::{Node, Parser, TreeCursor};

pub fn strip_trailing_commas(src: &str) -> String {
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
    collect(&mut cursor, &mut edits);
    apply(src, edits)
}

fn collect(cursor: &mut TreeCursor, edits: &mut Vec<(usize, usize)>) {
    let node = cursor.node();
    if !node.is_named() && is_comma(node) {
        if let Some(next) = node.next_sibling() {
            if is_closer(next) {
                edits.push((node.start_byte(), node.end_byte()));
            }
        }
    }

    if cursor.goto_first_child() {
        loop {
            collect(cursor, edits);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

fn is_comma(node: Node) -> bool {
    node.kind() == ","
}

fn is_closer(node: Node) -> bool {
    matches!(node.kind(), "]" | ")" | "}")
}

fn apply(src: &str, mut edits: Vec<(usize, usize)>) -> String {
    edits.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(src.len());
    let mut cursor = 0usize;
    for (start, end) in edits {
        if start < cursor {
            continue;
        }
        out.push_str(&src[cursor..start]);
        cursor = end;
    }
    out.push_str(&src[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_trailing_comma_removed() {
        assert_eq!(strip_trailing_commas("[1, 2, 3,]"), "[1, 2, 3]");
    }

    #[test]
    fn object_trailing_comma_removed() {
        assert_eq!(strip_trailing_commas("({a: 1, b: 2,})"), "({a: 1, b: 2})");
    }

    #[test]
    fn args_trailing_comma_removed() {
        assert_eq!(strip_trailing_commas("foo(1, 2,)"), "foo(1, 2)");
    }

    #[test]
    fn params_trailing_comma_removed() {
        assert_eq!(
            strip_trailing_commas("function f(a, b,) {}"),
            "function f(a, b) {}"
        );
    }

    #[test]
    fn no_trailing_comma_unchanged() {
        assert_eq!(strip_trailing_commas("[1, 2, 3]"), "[1, 2, 3]");
    }

    #[test]
    fn separator_commas_preserved() {
        assert_eq!(strip_trailing_commas("[1, 2, 3]"), "[1, 2, 3]");
        assert_eq!(strip_trailing_commas("foo(a, b)"), "foo(a, b)");
    }
}
