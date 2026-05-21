//! Fallback-acceptance helpers for the `R.reason instanceof Error ? … : <X>`
//! ternary recognized by [`super::parse_instanceof_error_ternary`]. The
//! accepted shapes for `<X>` mirror the inline-ternary catch-narrowing rule:
//! `String(REASON)`, `REASON?.toString()`, `REASON?.PROP ?? <literal>`, or a
//! bare string literal. `is_safe_default` and `string_literal_value` live
//! here too because they only appear inside this rule.

use tree_sitter::Node;

use crate::node_utils::{compact_node_text, first_named_child, node_text, raw_comparable_children};

pub(super) fn fallback_is_accepted(
    node: Node,
    src: &str,
    reason_text: &str,
    property: &str,
) -> bool {
    match node.kind() {
        "string" => true,
        "call_expression" => {
            is_string_call(node, src, reason_text) || is_optional_to_string(node, src, reason_text)
        }
        "binary_expression" => is_optional_prop_or_literal(node, src, reason_text, property),
        _ => false,
    }
}

fn is_string_call(call: Node, src: &str, reason_text: &str) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "identifier" || node_text(callee, src) != "String" {
        return false;
    }
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    let args: Vec<Node> = raw_comparable_children(arguments)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    if args.len() != 1 {
        return false;
    }
    compact_node_text(unwrap_parens(args[0]), src) == reason_text
}

fn is_optional_to_string(call: Node, src: &str, reason_text: &str) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression"
        || callee.child_by_field_name("optional_chain").is_none()
    {
        return false;
    }
    let Some(object) = callee.child_by_field_name("object") else {
        return false;
    };
    if compact_node_text(object, src) != reason_text {
        return false;
    }
    let Some(property) = callee.child_by_field_name("property") else {
        return false;
    };
    if node_text(property, src) != "toString" {
        return false;
    }
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    raw_comparable_children(arguments)
        .into_iter()
        .filter(|n| n.is_named())
        .count()
        == 0
}

fn is_optional_prop_or_literal(node: Node, src: &str, reason_text: &str, property: &str) -> bool {
    let Some(op) = node.child_by_field_name("operator") else {
        return false;
    };
    if node_text(op, src) != "??" {
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
    if right.kind() != "string" || left.kind() != "member_expression" {
        return false;
    }
    if left.child_by_field_name("optional_chain").is_none() {
        return false;
    }
    let Some(object) = left.child_by_field_name("object") else {
        return false;
    };
    if compact_node_text(object, src) != reason_text {
        return false;
    }
    let Some(prop) = left.child_by_field_name("property") else {
        return false;
    };
    node_text(prop, src) == property
}

/// Literal "safe defaults" the migration may substitute when the outer
/// status ternary takes the non-rejected branch: scalar literal, empty
/// container, or the bare `undefined` identifier.
pub(super) fn is_safe_default(node: Node, src: &str) -> bool {
    match node.kind() {
        "string" | "number" | "null" | "true" | "false" | "undefined" => true,
        "object" | "array" => {
            raw_comparable_children(node)
                .into_iter()
                .filter(|n| n.is_named())
                .count()
                == 0
        }
        "identifier" => node_text(node, src) == "undefined",
        _ => false,
    }
}

/// Extracts the contents of a quoted-string node (single, double, or
/// backtick-delimited). Returns `None` for non-string nodes or malformed
/// quoting. Used by the outer status check to compare `"rejected"`.
pub(super) fn string_literal_value(node: Node, src: &str) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
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

fn unwrap_parens(mut node: Node) -> Node {
    while node.kind() == "parenthesized_expression" {
        match first_named_child(node) {
            Some(child) => node = child,
            None => break,
        }
    }
    node
}
