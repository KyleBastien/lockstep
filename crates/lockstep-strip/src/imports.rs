//! Type-import handling.
//!
//! Cases:
//!   1. `import type { Foo } from 'x';`         → drop entire statement.
//!   2. `import type * as Foo from 'x';`        → drop entire statement.
//!   3. `import type Foo from 'x';`             → drop entire statement.
//!   4. `import { type Foo, bar } from 'x';`    → drop only the `type Foo` specifier.
//!   5. `export type { Foo } from 'x';`         → drop entire statement.
//!   6. `export { type Foo, bar } from 'x';`    → drop only the type spec.
//!
//! tree-sitter-typescript marks (1)/(2)/(3)/(5) by a child token whose kind is
//! `import` followed immediately by a `type` token (or `export`+`type`). For
//! mixed forms (4)/(6) it marks the individual `import_specifier` /
//! `export_specifier` with a leading `type` token child.

use tree_sitter::Node;

/// Returns true if `node` (an `import_statement` or `export_statement`) is a
/// pure type import/export — the entire statement should be dropped.
pub fn is_type_only_statement(node: Node, src: &str) -> bool {
    let kind = node.kind();
    if kind != "import_statement" && kind != "export_statement" {
        return false;
    }
    if all_specifiers_are_type_only(node, src) {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let text = child.utf8_text(src.as_bytes()).unwrap_or("");
        // Look for the keyword `type` appearing as a child token *before*
        // any `{ }` import clause. e.g. `import type { Foo }`.
        if child.is_named() {
            if child.kind() == "import_clause" || child.kind() == "export_clause" {
                return false; // hit the clause without seeing `type` → not pure-type
            }
            continue;
        }
        if text == "type" {
            return true;
        }
        if text == "{" {
            return false;
        }
    }
    false
}

fn all_specifiers_are_type_only(node: Node, src: &str) -> bool {
    let mut total = 0usize;
    let mut type_only = 0usize;
    let mut cursor = node.walk();
    for child in walk_descendants(node, &mut cursor) {
        if !is_specifier(child) {
            continue;
        }
        total += 1;
        if has_leading_type_token(child, src.as_bytes()) {
            type_only += 1;
        }
    }
    total > 0 && total == type_only
}

/// Returns the byte ranges (start, end) of individual `import_specifier` /
/// `export_specifier` children of `node` that are type-only (`type Foo`).
/// Used when the parent statement is *not* pure-type but mixes value and type
/// specifiers.
pub fn type_specifier_ranges(node: Node, src: &str) -> Vec<(usize, usize)> {
    let mut cursor = node.walk();
    let bytes = src.as_bytes();
    walk_descendants(node, &mut cursor)
        .into_iter()
        .filter(|d| is_specifier(*d))
        .filter(|d| has_leading_type_token(*d, bytes))
        .map(|d| specifier_range_with_trailing_comma(d, bytes))
        .collect()
}

fn is_specifier(node: Node) -> bool {
    matches!(node.kind(), "import_specifier" | "export_specifier")
}

fn has_leading_type_token(node: Node, src: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() {
            continue;
        }
        if child.utf8_text(src).unwrap_or("") == "type" {
            return true;
        }
    }
    false
}

fn specifier_range_with_trailing_comma(node: Node, src: &[u8]) -> (usize, usize) {
    let start = node.start_byte();
    let mut end = node.end_byte();
    let mut idx = end;
    while idx < src.len() && (src[idx] == b' ' || src[idx] == b'\t') {
        idx += 1;
    }
    if idx < src.len() && src[idx] == b',' {
        end = idx + 1;
    }
    (start, end)
}

fn walk_descendants<'a>(root: Node<'a>, cursor: &mut tree_sitter::TreeCursor<'a>) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    collect(root, cursor, &mut out);
    out
}

fn collect<'a>(node: Node<'a>, cursor: &mut tree_sitter::TreeCursor<'a>, out: &mut Vec<Node<'a>>) {
    out.push(node);
    if node.child_count() == 0 {
        return;
    }
    cursor.reset(node);
    if cursor.goto_first_child() {
        loop {
            collect(cursor.node(), &mut cursor.clone(), out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_first_statement<'a>(parser: &'a mut Parser, src: &'a str) -> tree_sitter::Tree {
        parser
            .set_language(&tree_sitter_typescript::language_typescript())
            .unwrap();
        parser.parse(src, None).unwrap()
    }

    fn with_first_statement<R>(src: &str, f: impl FnOnce(Node, &str) -> R) -> R {
        let mut parser = Parser::new();
        let tree = parse_first_statement(&mut parser, src);
        let stmt = tree.root_node().named_child(0).unwrap();
        f(stmt, src)
    }

    fn is_type_only_src(src: &str) -> bool {
        with_first_statement(src, is_type_only_statement)
    }

    fn type_specifier_range_count(src: &str) -> usize {
        with_first_statement(src, |stmt, src| type_specifier_ranges(stmt, src).len())
    }

    #[test]
    fn is_type_only_statement_detects_pure_type_imports() {
        assert!(is_type_only_src("import type { Foo } from 'x';"));
    }

    #[test]
    fn is_type_only_statement_rejects_value_imports() {
        assert!(!is_type_only_src("import { foo } from 'x';"));
    }

    #[test]
    fn type_specifier_ranges_finds_mixed_type_imports() {
        let src = "import { type Foo, bar } from 'x';";
        assert_eq!(
            type_specifier_range_count(src),
            1,
            "expected one type-prefixed specifier"
        );
    }

    #[test]
    fn is_type_only_statement_detects_braced_type_only_imports() {
        assert!(is_type_only_src("import { type Foo } from 'x';"));
    }

    #[test]
    fn type_specifier_ranges_empty_for_value_imports() {
        assert_eq!(
            type_specifier_range_count("import { foo, bar } from 'x';"),
            0
        );
    }
}
