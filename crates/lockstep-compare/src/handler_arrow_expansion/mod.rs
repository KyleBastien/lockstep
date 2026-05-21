//! Equivalence rule for arrow-callback body expansion in `Promise.allSettled`
//! result handlers.
//!
//! Gated on `allow_promise_settled_discrimination`. A common strict-TS shape
//! inflates a single-expression arrow callback into a block that extracts the
//! rejection reason via a status-narrowed const:
//!
//! ```text
//! handleResult(r, () => log(`failed: ${r.reason.message}`));            // base
//! handleResult(r, () => {                                               // head
//!   const reason = r.status === "rejected"
//!     ? r.reason instanceof Error ? r.reason.message : String(r.reason)
//!     : "";
//!   log(`failed: ${reason}`);
//! });
//! ```
//!
//! The rule registers `LOCAL ↔ RESULT.reason.PROP` via the existing
//! `CatchNarrowedLocal` channel and sub-walks the trailing statement against
//! the base body. The outer `status === "rejected"` guard is the
//! observability gate: `LOCAL` only diverges from `RESULT.reason.PROP` on a
//! fulfilled branch where base would have thrown a `TypeError` anyway. The
//! inner `instanceof Error ? .PROP : <fallback>` ternary mirrors the
//! catch-narrowing fallback shapes (`String(EXPR)`, `EXPR?.toString()`,
//! `EXPR?.PROP ?? <literal>`, or a bare string literal).

mod fallback;

use lockstep_core::Finding;
use tree_sitter::Node;

use crate::node_utils::{compact_node_text, first_named_child, node_text, raw_comparable_children};
use crate::walk::{walk, CatchNarrowedLocal, WalkCtx};
use fallback::{fallback_is_accepted, is_safe_default, string_literal_value};

/// Non-composable pre-empt. Fires when `head` is a `statement_block` and
/// `base` is a non-block node (an arrow body expression in source). Returns
/// `true` when the head matched and the trailing statement was sub-walked
/// against `base`.
pub(super) fn maybe_unwrap_handler_arrow_expansion(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    findings: &mut Vec<Finding>,
) -> bool {
    if !ctx.allow_promise_settled_discrimination {
        return false;
    }
    if head.kind() != "statement_block" || base.kind() == "statement_block" {
        return false;
    }
    let head_src = ctx.head_src;
    let head_stmts: Vec<Node> = raw_comparable_children(head)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    if head_stmts.len() != 2 {
        return false;
    }
    let Some(extract) = extract_settled_reason_const(head_stmts[0], head_src) else {
        return false;
    };
    let Some(tail_expr) = expression_statement_inner(head_stmts[1]) else {
        return false;
    };
    let base_expr = if base.kind() == "expression_statement" {
        first_named_child(base).unwrap_or(base)
    } else {
        base
    };

    let mut child_ctx = ctx.clone();
    child_ctx.catch_narrowed_locals.push(CatchNarrowedLocal {
        head_local: extract.local_name,
        err_name: extract.reason_target_text,
        property: extract.property,
    });
    walk(&child_ctx, base_expr, tail_expr, findings);
    true
}

struct ReasonExtract {
    local_name: String,
    /// Compact text of `RESULT.reason` (e.g. `"result.reason"`). Stored on
    /// the alias's `err_name` field; matched against the compact text of the
    /// outer object on the base member expression at leaf-pair resolution.
    reason_target_text: String,
    /// Property name accessed via `instanceof Error ? RESULT.reason.PROP : …`.
    property: String,
}

fn extract_settled_reason_const(stmt: Node, src: &str) -> Option<ReasonExtract> {
    if !matches!(stmt.kind(), "lexical_declaration" | "variable_declaration") {
        return None;
    }
    let declarator = sole_variable_declarator(stmt)?;
    let name = declarator.child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    let local_name = node_text(name, src);
    let value = unwrap_parens(declarator.child_by_field_name("value")?);
    let parts = parse_status_ternary(value, src)?;
    Some(ReasonExtract {
        local_name,
        reason_target_text: parts.reason_target_text,
        property: parts.property,
    })
}

struct StatusTernaryParts {
    reason_target_text: String,
    property: String,
}

/// Recognizes `R.status === "rejected" ? INNER : DEFAULT` (or the equivalent
/// `R.status !== "fulfilled" ? INNER : DEFAULT`). Verifies `DEFAULT` is a
/// safe scalar default and `INNER` is the canonical `R.reason instanceof
/// Error ? R.reason.PROP : <fallback>` ternary.
fn parse_status_ternary(node: Node, src: &str) -> Option<StatusTernaryParts> {
    if node.kind() != "ternary_expression" {
        return None;
    }
    let condition = unwrap_parens(node.child_by_field_name("condition")?);
    let result_name = match_rejected_status_check(condition, src)?;
    let consequence = unwrap_parens(node.child_by_field_name("consequence")?);
    let alternative = unwrap_parens(node.child_by_field_name("alternative")?);
    if !is_safe_default(alternative, src) {
        return None;
    }
    parse_instanceof_error_ternary(consequence, src, &result_name)
}

/// Accepts the two condition forms that select the `rejected` branch on the
/// consequence side: `R.status === "rejected"` and `R.status !== "fulfilled"`.
fn match_rejected_status_check(condition: Node, src: &str) -> Option<String> {
    if condition.kind() != "binary_expression" {
        return None;
    }
    let op = node_text(condition.child_by_field_name("operator")?, src);
    let tag = match op.as_str() {
        "===" => "rejected",
        "!==" => "fulfilled",
        _ => return None,
    };
    let left = unwrap_parens(condition.child_by_field_name("left")?);
    let right = unwrap_parens(condition.child_by_field_name("right")?);
    if let Some(name) = status_with_tag(left, right, src, tag) {
        return Some(name);
    }
    status_with_tag(right, left, src, tag)
}

fn status_with_tag(member: Node, literal: Node, src: &str, tag: &str) -> Option<String> {
    if member.kind() != "member_expression" {
        return None;
    }
    if member.child_by_field_name("optional_chain").is_some() {
        return None;
    }
    let object = member.child_by_field_name("object")?;
    if object.kind() != "identifier" {
        return None;
    }
    let property = member.child_by_field_name("property")?;
    if node_text(property, src) != "status" {
        return None;
    }
    if string_literal_value(literal, src)? != tag {
        return None;
    }
    Some(node_text(object, src))
}

/// Parses `R.reason instanceof Error ? R.reason.PROP : <fallback>` and
/// returns the compact text of `R.reason` plus the accessed property.
fn parse_instanceof_error_ternary(
    node: Node,
    src: &str,
    result_name: &str,
) -> Option<StatusTernaryParts> {
    if node.kind() != "ternary_expression" {
        return None;
    }
    let condition = unwrap_parens(node.child_by_field_name("condition")?);
    let reason_text = condition_instanceof_error_on_reason(condition, src, result_name)?;
    let consequence = unwrap_parens(node.child_by_field_name("consequence")?);
    let (cons_obj_text, cons_prop) = consequence_reason_member(consequence, src, result_name)?;
    if cons_obj_text != reason_text {
        return None;
    }
    let alternative = unwrap_parens(node.child_by_field_name("alternative")?);
    if !fallback_is_accepted(alternative, src, &reason_text, &cons_prop) {
        return None;
    }
    Some(StatusTernaryParts {
        reason_target_text: reason_text,
        property: cons_prop,
    })
}

fn condition_instanceof_error_on_reason(
    node: Node,
    src: &str,
    result_name: &str,
) -> Option<String> {
    if node.kind() != "binary_expression" {
        return None;
    }
    let op = node.child_by_field_name("operator")?;
    if node_text(op, src) != "instanceof" {
        return None;
    }
    let left = unwrap_parens(node.child_by_field_name("left")?);
    let right = unwrap_parens(node.child_by_field_name("right")?);
    if right.kind() != "identifier" || node_text(right, src) != "Error" {
        return None;
    }
    reason_member_text(left, src, result_name)
}

fn reason_member_text(node: Node, src: &str, result_name: &str) -> Option<String> {
    if node.kind() != "member_expression" {
        return None;
    }
    if node.child_by_field_name("optional_chain").is_some() {
        return None;
    }
    let object = node.child_by_field_name("object")?;
    if object.kind() != "identifier" || node_text(object, src) != result_name {
        return None;
    }
    let property = node.child_by_field_name("property")?;
    if node_text(property, src) != "reason" {
        return None;
    }
    Some(compact_node_text(node, src))
}

fn consequence_reason_member(node: Node, src: &str, result_name: &str) -> Option<(String, String)> {
    if node.kind() != "member_expression" {
        return None;
    }
    if node.child_by_field_name("optional_chain").is_some() {
        return None;
    }
    let object = unwrap_parens(node.child_by_field_name("object")?);
    let obj_text = reason_member_text(object, src, result_name)?;
    let property = node.child_by_field_name("property")?;
    Some((obj_text, node_text(property, src)))
}

fn expression_statement_inner(stmt: Node) -> Option<Node> {
    if stmt.kind() != "expression_statement" {
        return None;
    }
    first_named_child(stmt)
}

fn sole_variable_declarator(stmt: Node) -> Option<Node> {
    let declarators: Vec<Node> = raw_comparable_children(stmt)
        .into_iter()
        .filter(|c| c.kind() == "variable_declarator")
        .collect();
    if declarators.len() != 1 {
        return None;
    }
    Some(declarators[0])
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
