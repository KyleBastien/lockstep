//! Equivalence rule for head-inserted defensive logger guards.
//!
//! Gated on `allow_defensive_log_guard`. Treats head shape
//!
//! ```text
//! if (CACHE) {
//!     LOGGER.METHOD(CACHE, ...args);
//! }
//! ```
//!
//! as equivalent to a base shape that calls `LOGGER.METHOD(CACHE, ...args);`
//! unconditionally, where `METHOD` is one of the names listed in
//! `defensive_log_guard_methods` (default: `debug`, `info`, `warn`, `error`,
//! `trace`, `log`).
//!
//! **WARNING — observable behavior change.** If the logger has side effects
//! when called with a null/undefined first argument (recording a "null
//! logged" event, raising a typed error, etc.), wrapping the call in the
//! guard changes behavior. The rule defaults OFF.
//!
//! Directional: only head adds the wrap. Base-has-wrap / head-removed is
//! left to flag — the size delta points the other way and the rule never
//! fires there.

use lockstep_core::Finding;
use tree_sitter::Node;

use crate::node_utils::{first_named_child, node_text, raw_comparable_children};
use crate::walk::{walk, WalkCtx};

/// Statement-pair pre-empt: if head is an `if (CACHE) { LOGGER.METHOD(CACHE,
/// ...) }` wrap around a single logger call, unwrap and re-`walk` the inner
/// statement against `base`. Returns `true` when handled.
pub(super) fn maybe_unwrap_log_guard(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    findings: &mut Vec<Finding>,
) -> bool {
    if !ctx.allow_defensive_log_guard {
        return false;
    }
    if head.kind() != "if_statement" {
        return false;
    }
    if base.kind() != "expression_statement" {
        return false;
    }
    let Some(inner_stmt) = inner_logger_stmt_if_matches(ctx, head) else {
        return false;
    };
    walk(ctx, base, inner_stmt, findings);
    true
}

/// Returns the inner `expression_statement` when `head` matches the log-guard
/// shape; otherwise `None`.
fn inner_logger_stmt_if_matches<'a>(ctx: &WalkCtx, head: Node<'a>) -> Option<Node<'a>> {
    let condition = unwrap_parens(head.child_by_field_name("condition")?);
    let guard_text = guard_subject_compact_text(condition, ctx.head_src)?;
    let inner_stmt = sole_inner_statement(head.child_by_field_name("consequence")?)?;
    let call = expression_statement_call(inner_stmt)?;
    if !call_is_whitelisted_logger(ctx, call) {
        return None;
    }
    if !first_arg_matches(ctx, call, &guard_text) {
        return None;
    }
    Some(inner_stmt)
}

fn sole_inner_statement(consequence: Node) -> Option<Node> {
    if consequence.kind() != "statement_block" {
        return None;
    }
    let named: Vec<Node> = raw_comparable_children(consequence)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    if named.len() != 1 {
        return None;
    }
    let inner_stmt = named[0];
    if inner_stmt.kind() != "expression_statement" {
        return None;
    }
    Some(inner_stmt)
}

fn expression_statement_call(stmt: Node) -> Option<Node> {
    let call = first_named_child(stmt)?;
    if call.kind() != "call_expression" {
        return None;
    }
    Some(call)
}

fn call_is_whitelisted_logger(ctx: &WalkCtx, call: Node) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    let callee = unwrap_parens(callee);
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(method) = callee.child_by_field_name("property") else {
        return false;
    };
    let method_text = node_text(method, ctx.head_src);
    ctx.defensive_log_guard_methods
        .iter()
        .any(|m| m == &method_text)
}

fn first_arg_matches(ctx: &WalkCtx, call: Node, guard_text: &str) -> bool {
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    let arg_list: Vec<Node> = raw_comparable_children(arguments)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    if arg_list.is_empty() {
        return false;
    }
    let first_arg = unwrap_parens(arg_list[0]);
    guard_subject_compact_text(first_arg, ctx.head_src).as_deref() == Some(guard_text)
}

/// Accepts the same shapes as guard subjects: bare identifier or `this.PROP`
/// member expression. Returns compact text on match, `None` otherwise.
fn guard_subject_compact_text(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "identifier" => Some(node_text(node, src)),
        "member_expression" => {
            let object = node.child_by_field_name("object")?;
            if node_text(object, src) != "this" {
                return None;
            }
            let prop = node.child_by_field_name("property")?;
            Some(format!("this.{}", node_text(prop, src)))
        }
        _ => None,
    }
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
