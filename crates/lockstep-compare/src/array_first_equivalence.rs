//! Equivalence rule for the "first element of array, else null" idiom.
//!
//! Tier 1 (gated on `allow_array_first_element_or_null`) treats these base
//! shapes as equivalent to a head `EXPR[0] ?? null` (or optional-chained
//! `EXPR?.[0] ?? null`):
//!
//! * `EXPR.length > 0 ? EXPR[0] : null`
//! * `EXPR && EXPR.length > 0 ? EXPR[0] : null`
//! * `EXPR?.length > 0 ? EXPR[0] : null`
//!
//! Tier 2 (gated on `allow_array_first_element_or_null_loose`) additionally
//! accepts `EXPR[0] || null` and bare `EXPR[0]` as base shapes. These diverge
//! from `??` on falsy non-null values and explicit `undefined` respectively;
//! the rule is opt-in for that reason.

use tree_sitter::Node;

use crate::node_utils::{compact_node_text, first_named_child};
use crate::walk::{walk, WalkCtx};

pub(super) fn is_array_first_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    let base = unwrap_parens(base);
    let head = unwrap_parens(head);
    let Some(target) = nullish_null_target(head, ctx.head_src) else {
        return false;
    };
    if ctx.allow_array_first_element_or_null && tier1_base_matches(ctx, base, &target) {
        return true;
    }
    if ctx.allow_array_first_element_or_null_loose && tier2_base_matches(ctx, base, &target) {
        return true;
    }
    false
}

struct HeadTarget<'a> {
    object: Node<'a>,
    optional: bool,
}

fn nullish_null_target<'a>(node: Node<'a>, src: &str) -> Option<HeadTarget<'a>> {
    let lhs = nullish_null_lhs(node, src)?;
    Some(HeadTarget {
        object: lhs.child_by_field_name("object")?,
        optional: lhs.child_by_field_name("optional_chain").is_some(),
    })
}

fn nullish_null_lhs<'a>(node: Node<'a>, src: &str) -> Option<Node<'a>> {
    binary_null_fallback_lhs(node, "??", src)
}

fn binary_null_fallback_lhs<'a>(node: Node<'a>, op: &str, src: &str) -> Option<Node<'a>> {
    if node.kind() != "binary_expression" || !operator_is(node, op) {
        return None;
    }
    let right = unwrap_parens(node.child_by_field_name("right")?);
    if right.kind() != "null" {
        return None;
    }
    let left = unwrap_parens(node.child_by_field_name("left")?);
    if !is_zero_index_subscript(left, src) {
        return None;
    }
    Some(left)
}

fn tier1_base_matches(ctx: &WalkCtx, base: Node, head: &HeadTarget) -> bool {
    if base.kind() != "ternary_expression" {
        return false;
    }
    if !alternative_is_null(base) {
        return false;
    }
    let Some(consequence) = base.child_by_field_name("consequence") else {
        return false;
    };
    if !consequence_matches(ctx, unwrap_parens(consequence), head.object) {
        return false;
    }
    let Some(condition) = base.child_by_field_name("condition") else {
        return false;
    };
    condition_matches(ctx, unwrap_parens(condition), head.object, head.optional)
}

fn tier2_base_matches(ctx: &WalkCtx, base: Node, head: &HeadTarget) -> bool {
    if let Some(left) = or_null_lhs(base, ctx.base_src) {
        return subscript_matches(ctx, left, head);
    }
    if base.kind() == "subscript_expression" && is_zero_index_subscript(base, ctx.base_src) {
        return subscript_matches(ctx, base, head);
    }
    false
}

fn or_null_lhs<'a>(node: Node<'a>, src: &str) -> Option<Node<'a>> {
    binary_null_fallback_lhs(node, "||", src)
}

fn subscript_matches(ctx: &WalkCtx, node: Node, head: &HeadTarget) -> bool {
    let is_optional = node.child_by_field_name("optional_chain").is_some();
    if is_optional != head.optional {
        return false;
    }
    consequence_matches(ctx, node, head.object)
}

fn consequence_matches(ctx: &WalkCtx, node: Node, expected_obj: Node) -> bool {
    if !is_zero_index_subscript(node, ctx.base_src) {
        return false;
    }
    let Some(object) = node.child_by_field_name("object") else {
        return false;
    };
    exprs_equivalent(ctx, unwrap_parens(object), expected_obj)
}

fn condition_matches(ctx: &WalkCtx, cond: Node, expected_obj: Node, expect_optional: bool) -> bool {
    if cond.kind() != "binary_expression" {
        return false;
    }
    if operator_is(cond, ">") {
        return length_gt_zero(ctx, cond, expected_obj, expect_optional);
    }
    if operator_is(cond, "&&") {
        return and_length_check(ctx, cond, expected_obj, expect_optional);
    }
    false
}

fn length_gt_zero(ctx: &WalkCtx, cond: Node, expected_obj: Node, expect_optional: bool) -> bool {
    let Some(right) = cond.child_by_field_name("right") else {
        return false;
    };
    if !is_literal_zero(unwrap_parens(right), ctx.base_src) {
        return false;
    }
    let Some(left) = cond.child_by_field_name("left") else {
        return false;
    };
    is_length_access(ctx, unwrap_parens(left), expected_obj, expect_optional)
}

fn and_length_check(ctx: &WalkCtx, cond: Node, expected_obj: Node, expect_optional: bool) -> bool {
    let Some(left) = cond.child_by_field_name("left") else {
        return false;
    };
    if !exprs_equivalent(ctx, unwrap_parens(left), expected_obj) {
        return false;
    }
    let Some(right) = cond.child_by_field_name("right") else {
        return false;
    };
    let right = unwrap_parens(right);
    if right.kind() != "binary_expression" || !operator_is(right, ">") {
        return false;
    }
    length_gt_zero(ctx, right, expected_obj, expect_optional)
}

fn is_length_access(ctx: &WalkCtx, node: Node, expected_obj: Node, expect_optional: bool) -> bool {
    if node.kind() != "member_expression" {
        return false;
    }
    let Some(property) = node.child_by_field_name("property") else {
        return false;
    };
    if property.utf8_text(ctx.base_src.as_bytes()).unwrap_or("") != "length" {
        return false;
    }
    let is_optional = node.child_by_field_name("optional_chain").is_some();
    if is_optional != expect_optional {
        return false;
    }
    let Some(object) = node.child_by_field_name("object") else {
        return false;
    };
    exprs_equivalent(ctx, unwrap_parens(object), expected_obj)
}

fn is_zero_index_subscript(node: Node, src: &str) -> bool {
    if node.kind() != "subscript_expression" {
        return false;
    }
    let Some(index) = node.child_by_field_name("index") else {
        return false;
    };
    is_literal_zero(unwrap_parens(index), src)
}

fn alternative_is_null(ternary: Node) -> bool {
    ternary
        .child_by_field_name("alternative")
        .map(|alt| unwrap_parens(alt).kind() == "null")
        .unwrap_or(false)
}

fn is_literal_zero(node: Node, src: &str) -> bool {
    node.kind() == "number" && node.utf8_text(src.as_bytes()).unwrap_or("") == "0"
}

/// Fast path: byte-equal after whitespace strip. Walker fallback re-enters
/// `walk()` against the live `ctx`, letting cache aliases, transient locals,
/// and other equivalences feed in. Findings stay isolated to the local vec
/// so the outer pass is unaffected.
fn exprs_equivalent(ctx: &WalkCtx, a: Node, b: Node) -> bool {
    if compact_node_text(a, ctx.base_src) == compact_node_text(b, ctx.head_src) {
        return true;
    }
    let mut findings = Vec::new();
    walk(ctx, a, b, &mut findings);
    findings.is_empty()
}

fn operator_is(binary: Node, op: &str) -> bool {
    binary
        .child_by_field_name("operator")
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
