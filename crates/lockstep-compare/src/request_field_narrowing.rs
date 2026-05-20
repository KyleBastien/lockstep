//! Equivalence rule for strict-TS request-field narrowing.
//!
//! Gated on `allow_request_field_narrowing`. Treats a head block that opens
//! with the narrowing extraction
//!
//! ```text
//! const IDENT = "PROP" in OBJ && typeof OBJ.PROP === "TYPE"
//!     ? OBJ.PROP
//!     : undefined;
//! ```
//!
//! as a no-op against a base block that uses `OBJ.PROP` directly. Subsequent
//! head references to `IDENT` then compare equal to base `OBJ.PROP` member
//! accesses for the duration of the block.
//!
//! Strict TS forces this shape when the enclosing generic does not admit
//! property access on the request type; the alternative is widening the base
//! class generic everywhere.
//!
//! Directional: only head adds the narrowing local. The reverse (base
//! narrows, head uses bare member access) is left to flag.

use tree_sitter::Node;

use crate::node_utils::{compact_node_text, first_named_child, node_text, raw_comparable_children};
use crate::walk::{NarrowedRequestField, WalkCtx};

/// Returns `true` when the head identifier resolves to a registered narrowing
/// alias matching the base member expression.
pub(super) fn is_narrowed_request_field_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if head.kind() != "identifier" || base.kind() != "member_expression" {
        return false;
    }
    let head_text = node_text(head, ctx.head_src);
    let Some(narrow) = ctx
        .narrowed_request_fields
        .iter()
        .find(|n| n.head_name == head_text)
    else {
        return false;
    };
    let Some(object) = base.child_by_field_name("object") else {
        return false;
    };
    let Some(property) = base.child_by_field_name("property") else {
        return false;
    };
    let base_obj_text = compact_node_text(object, ctx.base_src);
    let base_prop_text = node_text(property, ctx.base_src);
    narrow.base_object == base_obj_text && narrow.base_property == base_prop_text
}

/// Composable variant: detects request-field narrowing declarations in the
/// given head block and registers each scoped alias onto `child_ctx`. Returns
/// `true` when at least one alias was registered.
pub(super) fn apply_request_field_narrowing(
    child_ctx: &mut WalkCtx,
    base: Node,
    head: Node,
) -> bool {
    if !child_ctx.allow_request_field_narrowing {
        return false;
    }
    if base.kind() != "statement_block" || head.kind() != "statement_block" {
        return false;
    }
    let head_src = child_ctx.head_src;
    let head_stmts: Vec<Node> = raw_comparable_children(head)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    let mut applied = false;
    for stmt in &head_stmts {
        let Some(narrow) = extract_narrow(*stmt, head_src) else {
            continue;
        };
        if !head_stmts.iter().any(|s| {
            s.start_byte() != stmt.start_byte() && uses_identifier(*s, head_src, &narrow.head_name)
        }) {
            continue;
        }
        child_ctx.ignored_head_starts.push(stmt.start_byte());
        child_ctx
            .narrowed_request_fields
            .push(NarrowedRequestField {
                head_name: narrow.head_name,
                base_object: narrow.base_object,
                base_property: narrow.base_property,
            });
        applied = true;
    }
    applied
}

struct Narrow {
    head_name: String,
    base_object: String,
    base_property: String,
}

fn extract_narrow(stmt: Node, src: &str) -> Option<Narrow> {
    let (head_name, ternary) = parse_decl_ternary(stmt, src)?;
    let (base_object, base_property) = parse_ternary_target(ternary, src)?;
    if !narrow_condition_matches(ternary, src, &base_object, &base_property) {
        return None;
    }
    Some(Narrow {
        head_name,
        base_object,
        base_property,
    })
}

fn parse_decl_ternary<'a>(stmt: Node<'a>, src: &str) -> Option<(String, Node<'a>)> {
    if !matches!(stmt.kind(), "lexical_declaration" | "variable_declaration") {
        return None;
    }
    let declarators: Vec<Node> = raw_comparable_children(stmt)
        .into_iter()
        .filter(|c| c.kind() == "variable_declarator")
        .collect();
    if declarators.len() != 1 {
        return None;
    }
    let declarator = declarators[0];
    let name = declarator.child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    let head_name = node_text(name, src);
    let value = unwrap_parens(declarator.child_by_field_name("value")?);
    if value.kind() != "ternary_expression" {
        return None;
    }
    Some((head_name, value))
}

/// Extracts `(OBJ_text, PROP_text)` from a ternary whose alternative is
/// `undefined` and consequence is `OBJ.PROP`. Returns `None` otherwise.
fn parse_ternary_target(ternary: Node, src: &str) -> Option<(String, String)> {
    let alternative = unwrap_parens(ternary.child_by_field_name("alternative")?);
    if !is_undefined_literal(alternative, src) {
        return None;
    }
    let consequence = unwrap_parens(ternary.child_by_field_name("consequence")?);
    if consequence.kind() != "member_expression" {
        return None;
    }
    let cons_obj = unwrap_parens(consequence.child_by_field_name("object")?);
    let cons_prop = consequence.child_by_field_name("property")?;
    Some((compact_node_text(cons_obj, src), node_text(cons_prop, src)))
}

fn narrow_condition_matches(ternary: Node, src: &str, obj: &str, prop: &str) -> bool {
    let Some(condition) = ternary.child_by_field_name("condition") else {
        return false;
    };
    let condition = unwrap_parens(condition);
    if condition.kind() != "binary_expression" || !operator_is(condition, "&&") {
        return false;
    }
    let Some(left) = condition.child_by_field_name("left") else {
        return false;
    };
    let Some(right) = condition.child_by_field_name("right") else {
        return false;
    };
    matches_in_check(unwrap_parens(left), src, obj, prop)
        && matches_typeof_check(unwrap_parens(right), src, obj, prop)
}

fn matches_in_check(node: Node, src: &str, expected_obj: &str, expected_prop: &str) -> bool {
    if node.kind() != "binary_expression" || !operator_is(node, "in") {
        return false;
    }
    let Some(left) = node.child_by_field_name("left") else {
        return false;
    };
    let Some(right) = node.child_by_field_name("right") else {
        return false;
    };
    let left = unwrap_parens(left);
    let right = unwrap_parens(right);
    if left.kind() != "string" {
        return false;
    }
    if string_value(left, src).as_deref() != Some(expected_prop) {
        return false;
    }
    compact_node_text(right, src) == expected_obj
}

fn matches_typeof_check(node: Node, src: &str, expected_obj: &str, expected_prop: &str) -> bool {
    if node.kind() != "binary_expression" {
        return false;
    }
    let Some(op) = node.child_by_field_name("operator") else {
        return false;
    };
    let op_text = node_text(op, src);
    if op_text != "===" {
        return false;
    }
    let Some(left) = node.child_by_field_name("left") else {
        return false;
    };
    let Some(right) = node.child_by_field_name("right") else {
        return false;
    };
    let left = unwrap_parens(left);
    let right = unwrap_parens(right);
    if right.kind() != "string" {
        return false;
    }
    if left.kind() != "unary_expression" {
        return false;
    }
    if !unary_is_typeof(left, src) {
        return false;
    }
    let arg = unwrap_parens(unary_argument(left).unwrap_or(left));
    let combined = format!("{expected_obj}.{expected_prop}");
    compact_node_text(arg, src) == combined
}

fn unary_is_typeof(node: Node, src: &str) -> bool {
    if let Some(op) = node.child_by_field_name("operator") {
        return node_text(op, src) == "typeof";
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .filter(|c| !c.is_named())
        .any(|c| node_text(c, src) == "typeof")
}

fn unary_argument(node: Node) -> Option<Node> {
    if let Some(arg) = node.child_by_field_name("argument") {
        return Some(arg);
    }
    first_named_child(node)
}

fn is_undefined_literal(node: Node, src: &str) -> bool {
    if node.kind() == "undefined" {
        return true;
    }
    node.kind() == "identifier" && node_text(node, src) == "undefined"
}

fn string_value(node: Node, src: &str) -> Option<String> {
    let text = node_text(node, src);
    let bytes = text.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    if first != last || !matches!(first, b'\'' | b'"' | b'`') {
        return None;
    }
    Some(text[1..text.len() - 1].to_string())
}

fn uses_identifier(node: Node, src: &str, name: &str) -> bool {
    if node.kind() == "identifier" && node_text(node, src) == name {
        return true;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .any(|child| uses_identifier(child, src, name))
}

fn operator_is(node: Node, op: &str) -> bool {
    node.child_by_field_name("operator")
        .is_some_and(|n| n.kind() == op)
}

fn unwrap_parens(mut node: Node) -> Node {
    while node.kind() == "parenthesized_expression" {
        match first_named_child(node) {
            Some(child) => node = child,
            None => break,
        }
    }
    node
}
