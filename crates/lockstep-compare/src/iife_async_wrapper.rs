//! Equivalence rule for the sync-method-wraps-async-IIFE pattern.
//!
//! Gated on `allow_iife_async_wrapper`. Treats a head method
//!
//! ```text
//! get() { return (async () => { BODY })(); }
//! ```
//!
//! as equivalent to a base async callable whose body is `BODY`. Strict TS
//! migrations adopt this shape when the head method needs a branded return
//! type (`Promise<X> & { __opts }`) that a bare `async` declaration cannot
//! satisfy. After strip removes the type annotation, only the IIFE wrapper
//! is visible to the walker — this rule cancels it.
//!
//! Directional: base async + head sync-with-async-IIFE only. The reverse
//! (base sync, head async IIFE) is left to flag as a real behavior change.

use lockstep_core::Finding;
use tree_sitter::Node;

use crate::node_utils::{
    find_direct_child, first_named_child, node_text, raw_comparable_children, statement_block,
};
use crate::walk::{walk, WalkCtx};

/// Outcome of attempting the IIFE pre-empt.
pub(super) enum IifeAsyncOutcome<'a> {
    /// Not the pattern; caller falls through.
    NotApplicable,
    /// The pattern matched; the inner IIFE body should be compared against
    /// the base callable's body using the standard walker.
    Match {
        base_body: Node<'a>,
        head_inner_body: Node<'a>,
    },
}

/// Inspects a constructor-assigned base callable and a head class-method node
/// to see if the IIFE wrapper rule applies.
pub(super) fn iife_async_outcome<'a>(
    ctx: &WalkCtx,
    base_function: Node<'a>,
    head_method: Node<'a>,
) -> IifeAsyncOutcome<'a> {
    if !ctx.allow_iife_async_wrapper {
        return IifeAsyncOutcome::NotApplicable;
    }
    if !async_flag(base_function, ctx.base_src) {
        return IifeAsyncOutcome::NotApplicable;
    }
    if async_flag(head_method, ctx.head_src) {
        return IifeAsyncOutcome::NotApplicable;
    }
    let Some(head_body) = statement_block(head_method) else {
        return IifeAsyncOutcome::NotApplicable;
    };
    let Some(inner_body) = head_iife_inner_body(head_body, ctx.head_src) else {
        return IifeAsyncOutcome::NotApplicable;
    };
    let Some(base_body) = callable_body(base_function) else {
        return IifeAsyncOutcome::NotApplicable;
    };
    IifeAsyncOutcome::Match {
        base_body,
        head_inner_body: inner_body,
    }
}

/// Recurses into the unwrapped IIFE body against the base callable body using
/// the standard walker. Composition with other equivalence rules happens
/// naturally because we re-enter `walk()`.
pub(super) fn walk_iife_async_bodies(
    ctx: &WalkCtx,
    base_body: Node,
    head_inner_body: Node,
    findings: &mut Vec<Finding>,
) {
    walk(ctx, base_body, head_inner_body, findings);
}

/// Returns the body node of the inner async IIFE if `body` is the singular
/// statement_block `{ return (async () => INNER)(); }`.
fn head_iife_inner_body<'a>(body: Node<'a>, src: &str) -> Option<Node<'a>> {
    let ret = sole_return_statement(body)?;
    let call = unwrap_parens(first_named_child(ret)?);
    if call.kind() != "call_expression" || !call_has_no_arguments(call) {
        return None;
    }
    let callee = unwrap_parens(call.child_by_field_name("function")?);
    if !is_async_zero_argument_callable(callee, src) {
        return None;
    }
    callable_body(callee)
}

fn sole_return_statement(body: Node) -> Option<Node> {
    if body.kind() != "statement_block" {
        return None;
    }
    let mut named = raw_comparable_children(body)
        .into_iter()
        .filter(|child| child.is_named());
    let only = named.next()?;
    if named.next().is_some() {
        return None;
    }
    if only.kind() != "return_statement" {
        return None;
    }
    Some(only)
}

fn call_has_no_arguments(call: Node) -> bool {
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    raw_comparable_children(arguments)
        .into_iter()
        .filter(|child| child.is_named())
        .count()
        == 0
}

fn is_async_zero_argument_callable(callee: Node, src: &str) -> bool {
    matches!(callee.kind(), "arrow_function" | "function_expression")
        && async_flag(callee, src)
        && is_zero_argument_iife(callee)
}

fn is_zero_argument_iife(callable: Node) -> bool {
    let Some(params) = find_direct_child(callable, "formal_parameters") else {
        return matches!(callable.kind(), "arrow_function");
    };
    raw_comparable_children(params)
        .into_iter()
        .filter(|child| child.is_named())
        .count()
        == 0
}

fn callable_body(node: Node) -> Option<Node> {
    node.child_by_field_name("body").or_else(|| {
        raw_comparable_children(node)
            .into_iter()
            .rev()
            .find(|child| child.kind() != "formal_parameters")
    })
}

fn async_flag(node: Node, src: &str) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .filter(|child| !child.is_named())
        .any(|child| node_text(child, src) == "async");
    found
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
