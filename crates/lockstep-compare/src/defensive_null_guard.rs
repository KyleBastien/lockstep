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

use tree_sitter::Node;

use crate::node_utils::{first_named_child, node_text, raw_comparable_children};
use crate::walk::WalkCtx;

/// Composable variant: scans the head statement_block for any defensive-null
/// guard and, if found, pushes its byte range to `child_ctx.ignored_head_starts`.
/// Adjacency to the protected mutation is no longer required — other block
/// rules (e.g. `non_null_alias_local`) may also strip statements before
/// `walk_regular` runs.
///
/// Returns `true` when a guard was stripped.
pub(super) fn apply_defensive_null_guard(child_ctx: &mut WalkCtx, base: Node, head: Node) -> bool {
    if !child_ctx.allow_defensive_null_guard {
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
    let guards: Vec<&Node> = head_stmts
        .iter()
        .filter(|s| is_defensive_null_guard(**s, head_src))
        .collect();
    if guards.len() != 1 {
        return false;
    }
    child_ctx.ignored_head_starts.push(guards[0].start_byte());
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
