//! Equivalence rule for head-inserted defensive null guards.
//!
//! Gated on `allow_defensive_null_guard`. Treats a head block that contains
//! exactly one extra statement of the shape
//!
//! ```text
//! if (!CACHE) {
//!     logErrorCall(...);
//!     return LITERAL;
//! }
//! ```
//!
//! as equivalent to a base block that omits the guard.
//!
//! **WARNING — observable behavior change.** Without the guard, base code
//! throws (e.g. `Object.assign(null, ...)` raises `TypeError`); with the
//! guard, head returns an error literal instead. Callers that catch the
//! throw may behave differently. This rule defaults OFF and should stay
//! OFF unless the migration was explicitly designed around the swap.
//!
//! Directional: only head adds the guard. If base has the guard and head
//! removed it, the size delta points the other way and the rule never
//! fires.

use lockstep_core::Finding;
use tree_sitter::Node;

use crate::node_utils::{first_named_child, node_text, raw_comparable_children};
use crate::walk::{walk_regular, WalkCtx};

/// When a head statement_block contains exactly one extra statement that
/// matches the defensive-guard shape, drops it via `ignored_head_starts`
/// and re-runs the regular block walk. Returns `true` when the block was
/// consumed.
pub(super) fn try_defensive_null_guard(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    findings: &mut Vec<Finding>,
) -> bool {
    if !ctx.allow_defensive_null_guard {
        return false;
    }
    if base.kind() != "statement_block" || head.kind() != "statement_block" {
        return false;
    }
    let base_stmts: Vec<Node> = raw_comparable_children(base)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    let head_stmts: Vec<Node> = raw_comparable_children(head)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    if head_stmts.len() != base_stmts.len() + 1 {
        return false;
    }
    let guards: Vec<&Node> = head_stmts
        .iter()
        .filter(|s| is_defensive_null_guard(**s, ctx.head_src))
        .collect();
    if guards.len() != 1 {
        return false;
    }
    let mut child_ctx = ctx.clone();
    child_ctx.ignored_head_starts.push(guards[0].start_byte());
    walk_regular(&child_ctx, base, head, findings);
    true
}

fn is_defensive_null_guard(stmt: Node, src: &str) -> bool {
    if stmt.kind() != "if_statement" {
        return false;
    }
    let Some(condition) = stmt.child_by_field_name("condition") else {
        return false;
    };
    let condition = unwrap_parens(condition);
    if condition.kind() != "unary_expression" {
        return false;
    }
    if !unary_is_negation(condition, src) {
        return false;
    }
    let Some(consequence) = stmt.child_by_field_name("consequence") else {
        return false;
    };
    if consequence.kind() != "statement_block" {
        return false;
    }
    let body_stmts: Vec<Node> = raw_comparable_children(consequence)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    if body_stmts.len() != 2 {
        return false;
    }
    if body_stmts[0].kind() != "expression_statement" {
        return false;
    }
    let Some(call) = first_named_child(body_stmts[0]) else {
        return false;
    };
    if call.kind() != "call_expression" {
        return false;
    }
    if body_stmts[1].kind() != "return_statement" {
        return false;
    }
    let Some(ret_val) = first_named_child(body_stmts[1]) else {
        return false;
    };
    matches!(
        ret_val.kind(),
        "string" | "number" | "null" | "undefined" | "true" | "false"
    )
}

fn unary_is_negation(node: Node, src: &str) -> bool {
    if let Some(op) = node.child_by_field_name("operator") {
        return node_text(op, src) == "!";
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .filter(|c| !c.is_named())
        .any(|c| node_text(c, src) == "!")
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
