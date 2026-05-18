//! Class-field handling.
//!
//! Specifically: `foo!: T;` (definite-assignment, no initializer) and
//! `declare foo: T;` (declared-only) — these have no JavaScript equivalent
//! and must drop entire.
//!
//! v1 also flags constructor parameter properties (`constructor(public x: T)`)
//! because mechanically synthesizing the `this.x = x` they imply is error-prone
//! across getter/setter ordering.

use tree_sitter::Node;

/// A `public_field_definition` (TS field decl) that exists *only* as a type
/// hint — no initializer and no JS-emitting effect. Caller should drop the
/// entire field's byte range.
pub fn is_typeonly_field(node: Node, src: &str) -> bool {
    if node.kind() != "public_field_definition" {
        return false;
    }
    let mut cursor = node.walk();
    let mut has_initializer = false;
    let mut has_declare_modifier = false;
    let bytes = src.as_bytes();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "=" => has_initializer = true,
            "declare" => has_declare_modifier = true,
            _ => {}
        }
        // Look for the definite-assignment marker `!` on the name.
        let txt = child.utf8_text(bytes).unwrap_or("");
        if txt == "!" && !child.is_named() {
            // Sometimes parsed as `!: T`; still no initializer required.
        }
    }
    has_declare_modifier || !has_initializer
}

/// `constructor(public x: T)` parameter property. v1 rejects.
pub fn is_parameter_property(node: Node) -> bool {
    if node.kind() != "required_parameter" && node.kind() != "optional_parameter" {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "accessibility_modifier" | "readonly" | "override_modifier"
        ) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::language_typescript())
            .unwrap();
        parser.parse(src, None).unwrap()
    }

    fn find_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn is_typeonly_field_true_for_definite_assignment() {
        let src = "class C { x!: number; }";
        let tree = parse(src);
        let field = find_kind(tree.root_node(), "public_field_definition").unwrap();
        assert!(is_typeonly_field(field, src));
    }

    #[test]
    fn is_typeonly_field_false_for_initialized_field() {
        let src = "class C { x: number = 0; }";
        let tree = parse(src);
        let field = find_kind(tree.root_node(), "public_field_definition").unwrap();
        assert!(!is_typeonly_field(field, src));
    }

    #[test]
    fn is_parameter_property_true_with_modifier() {
        let src = "class C { constructor(public x: number) {} }";
        let tree = parse(src);
        let param = find_kind(tree.root_node(), "required_parameter").unwrap();
        assert!(is_parameter_property(param));
    }

    #[test]
    fn is_parameter_property_false_without_modifier() {
        let src = "class C { constructor(x: number) {} }";
        let tree = parse(src);
        let param = find_kind(tree.root_node(), "required_parameter").unwrap();
        assert!(!is_parameter_property(param));
    }
}
