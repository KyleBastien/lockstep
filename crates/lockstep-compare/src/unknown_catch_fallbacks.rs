//! Fallback-arm recognizers for the `unknown_catch_narrowing` ternary rule.
//!
//! Pure recognizers — no walker state, no findings. Each function answers
//! "does this AST node match a documented stringify-the-caught-value
//! fallback shape?" given the `catch` binding name and (where relevant)
//! the consequence's accessed property.
//!
//! Kept in its own file so `unknown_catch_narrowing.rs` stays focused on
//! orchestration (block-strip + leaf-pair + scope checks).

use tree_sitter::Node;

use crate::node_utils::{first_named_child, node_text, raw_comparable_children};

/// Top-level entry. Returns `true` when `node` is any of the accepted
/// fallback shapes for the ternary's alternative arm.
pub(super) fn alternative_is_accepted(
    node: Node,
    src: &str,
    err_name: &str,
    property: &str,
) -> bool {
    match node.kind() {
        "string" => true,
        "call_expression" => alternative_is_accepted_call(node, src, err_name),
        "binary_expression" => alternative_is_accepted_binary(node, src, err_name, property),
        "ternary_expression" => alternative_is_typeof_string_ternary(node, src, err_name),
        _ => false,
    }
}

fn alternative_is_accepted_call(node: Node, src: &str, err_name: &str) -> bool {
    alternative_is_string_call(node, src, err_name)
        || alternative_is_optional_to_string(node, src, err_name)
        || alternative_is_string_call_with_nullish(node, src, err_name)
}

fn alternative_is_accepted_binary(node: Node, src: &str, err_name: &str, property: &str) -> bool {
    alternative_is_optional_prop_or_literal(node, src, err_name, property)
        || alternative_is_chained_optional_prop_or_literal(node, src, err_name)
}

/// `String(ERR)` — `call_expression` with callee `String`, single arg `ERR`.
fn alternative_is_string_call(call: Node, src: &str, err_name: &str) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "identifier" || node_text(callee, src) != "String" {
        return false;
    }
    let Some(arg) = sole_named_argument(call) else {
        return false;
    };
    let arg = unwrap_parens(arg);
    arg.kind() == "identifier" && node_text(arg, src) == err_name
}

/// `ERR?.toString()` — optional-chain member with property `toString`
/// called with no arguments.
fn alternative_is_optional_to_string(call: Node, src: &str, err_name: &str) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if !is_optional_member_with_property(callee, src, err_name, "toString") {
        return false;
    }
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    arguments_are_empty(arguments)
}

/// `ERR?.PROP ?? <string-literal>` — left optional-chain member access on
/// `ERR` to `PROP`, right a string literal.
fn alternative_is_optional_prop_or_literal(
    node: Node,
    src: &str,
    err_name: &str,
    property: &str,
) -> bool {
    let Some((left, right)) = nullish_pair(node, src) else {
        return false;
    };
    if right.kind() != "string" {
        return false;
    }
    if !is_optional_member_on_err(left, src, err_name) {
        return false;
    }
    let Some(prop_node) = left.child_by_field_name("property") else {
        return false;
    };
    node_text(prop_node, src) == property
}

/// `typeof ERR === "string" ? ERR : <string-literal>` — typeof-narrowed
/// ternary with a string-literal alternative.
fn alternative_is_typeof_string_ternary(node: Node, src: &str, err_name: &str) -> bool {
    let Some(condition) = node.child_by_field_name("condition") else {
        return false;
    };
    let Some(consequence) = node.child_by_field_name("consequence") else {
        return false;
    };
    let Some(alternative) = node.child_by_field_name("alternative") else {
        return false;
    };
    if !typeof_equals_string(unwrap_parens(condition), src, err_name) {
        return false;
    }
    let cons = unwrap_parens(consequence);
    if cons.kind() != "identifier" || node_text(cons, src) != err_name {
        return false;
    }
    unwrap_parens(alternative).kind() == "string"
}

/// `String(ERR ?? "...")` — `String(...)` with a `??` arg whose left is
/// `ERR` and right is a string literal.
fn alternative_is_string_call_with_nullish(call: Node, src: &str, err_name: &str) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "identifier" || node_text(callee, src) != "String" {
        return false;
    }
    let Some(arg) = sole_named_argument(call) else {
        return false;
    };
    nullish_err_with_string_default(unwrap_parens(arg), src, err_name)
}

/// `ERR?.PROP_A ?? ERR?.PROP_B ?? "literal"` — `??` chain whose leaves are
/// optional-chain accesses on `ERR` and at least one terminal string literal.
/// Accepts arbitrary `??` nesting (chains are left-associative in JS).
fn alternative_is_chained_optional_prop_or_literal(
    node: Node,
    src: &str,
    err_name: &str,
) -> bool {
    if nullish_pair(node, src).is_none() {
        return false;
    }
    let mut counts = ChainLeafCounts::default();
    if !walk_chain_leaves(node, src, err_name, &mut counts) {
        return false;
    }
    counts.optional_member_leaves >= 1 && counts.string_leaves >= 1
}

#[derive(Default)]
struct ChainLeafCounts {
    optional_member_leaves: usize,
    string_leaves: usize,
}

fn walk_chain_leaves(
    node: Node,
    src: &str,
    err_name: &str,
    counts: &mut ChainLeafCounts,
) -> bool {
    if node.kind() == "string" {
        counts.string_leaves += 1;
        return true;
    }
    if is_optional_member_on_err(node, src, err_name) {
        counts.optional_member_leaves += 1;
        return true;
    }
    let Some((left, right)) = nullish_pair(node, src) else {
        return false;
    };
    walk_chain_leaves(left, src, err_name, counts)
        && walk_chain_leaves(right, src, err_name, counts)
}

fn nullish_err_with_string_default(node: Node, src: &str, err_name: &str) -> bool {
    let Some((left, right)) = nullish_pair(node, src) else {
        return false;
    };
    if left.kind() != "identifier" || node_text(left, src) != err_name {
        return false;
    }
    right.kind() == "string"
}

fn typeof_equals_string(node: Node, src: &str, err_name: &str) -> bool {
    if node.kind() != "binary_expression" {
        return false;
    }
    let Some(op) = node.child_by_field_name("operator") else {
        return false;
    };
    let op_text = node_text(op, src);
    if op_text != "===" && op_text != "==" {
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
    if !unary_typeof_of_err(left, src, err_name) {
        return false;
    }
    right.kind() == "string" && string_literal_value(right, src) == "string"
}

fn unary_typeof_of_err(node: Node, src: &str, err_name: &str) -> bool {
    if node.kind() != "unary_expression" {
        return false;
    }
    let Some(operator) = node.child_by_field_name("operator") else {
        return false;
    };
    if node_text(operator, src) != "typeof" {
        return false;
    }
    let Some(arg) = node.child_by_field_name("argument") else {
        return false;
    };
    let arg = unwrap_parens(arg);
    arg.kind() == "identifier" && node_text(arg, src) == err_name
}

fn string_literal_value(node: Node, src: &str) -> String {
    let raw = node_text(node, src);
    raw.trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .to_string()
}

fn nullish_pair<'a>(node: Node<'a>, src: &str) -> Option<(Node<'a>, Node<'a>)> {
    if node.kind() != "binary_expression" {
        return None;
    }
    let op = node.child_by_field_name("operator")?;
    if node_text(op, src) != "??" {
        return None;
    }
    let left = unwrap_parens(node.child_by_field_name("left")?);
    let right = unwrap_parens(node.child_by_field_name("right")?);
    Some((left, right))
}

fn is_optional_member_on_err(node: Node, src: &str, err_name: &str) -> bool {
    if node.kind() != "member_expression" {
        return false;
    }
    if node.child_by_field_name("optional_chain").is_none() {
        return false;
    }
    let Some(object) = node.child_by_field_name("object") else {
        return false;
    };
    object.kind() == "identifier" && node_text(object, src) == err_name
}

fn is_optional_member_with_property(
    node: Node,
    src: &str,
    err_name: &str,
    property_name: &str,
) -> bool {
    if !is_optional_member_on_err(node, src, err_name) {
        return false;
    }
    let Some(property) = node.child_by_field_name("property") else {
        return false;
    };
    node_text(property, src) == property_name
}

fn sole_named_argument(call: Node) -> Option<Node> {
    let arguments = call.child_by_field_name("arguments")?;
    let args: Vec<Node> = raw_comparable_children(arguments)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    if args.len() != 1 {
        return None;
    }
    Some(args[0])
}

fn arguments_are_empty(arguments: Node) -> bool {
    raw_comparable_children(arguments)
        .into_iter()
        .filter(|n| n.is_named())
        .count()
        == 0
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
