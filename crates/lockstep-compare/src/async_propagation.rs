//! Equivalence rule for sync→async callable widening with await injection.
//!
//! Gated on `allow_async_propagation`. Accepts:
//!
//! ```text
//! // base
//! execute() { const r = this.validate(); return { ...r }; }
//! // head
//! async execute() { const r = await this.validate(); return { ...r }; }
//! ```
//!
//! Strict TS migrations adopt this when subclass overrides widen the return
//! type to `Promise<T>` — the base's sync caller would otherwise spread a
//! `Promise` object into `{}`, a latent JS bug TS makes visible.
//!
//! Directional: base sync → head async with at least one new `await` only.
//! The reverse (base async, head sync) is a real regression and is never
//! absorbed. Without the await-injection guard, this rule would silently
//! widen any `async` flip, which is too permissive.

use lockstep_core::Finding;
use tree_sitter::Node;

use crate::node_utils::{node_text, raw_comparable_children, statement_block};
use crate::walk::{walk, WalkCtx};

/// Detects an async-propagation callable pair and (if matched) compares the
/// bodies with `async_propagation_active = true`. Returns `true` when the
/// pair was consumed; the caller must not fall through.
pub(super) fn try_callable_async_propagation(
    ctx: &WalkCtx,
    base_callable: Node,
    head_callable: Node,
    findings: &mut Vec<Finding>,
) -> bool {
    if !ctx.allow_async_propagation {
        return false;
    }
    if async_flag(base_callable, ctx.base_src) || !async_flag(head_callable, ctx.head_src) {
        return false;
    }
    let Some(base_body) = callable_body(base_callable) else {
        return false;
    };
    let Some(head_body) = callable_body(head_callable) else {
        return false;
    };
    if !contains_await(head_body) {
        return false;
    }
    let mut child_ctx = ctx.clone();
    child_ctx.async_propagation_active = true;
    walk(&child_ctx, base_body, head_body, findings);
    true
}

/// Unwraps a head `await_expression` so its argument compares against the
/// base subtree, when `async_propagation_active` is set on `ctx`.
pub(super) fn maybe_unwrap_await(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    findings: &mut Vec<Finding>,
) -> bool {
    if !ctx.async_propagation_active {
        return false;
    }
    if head.kind() != "await_expression" {
        return false;
    }
    let Some(arg) = await_argument(head) else {
        return false;
    };
    walk(ctx, base, arg, findings);
    true
}

fn await_argument(node: Node) -> Option<Node> {
    if let Some(arg) = node.child_by_field_name("argument") {
        return Some(arg);
    }
    raw_comparable_children(node)
        .into_iter()
        .find(|c| c.is_named())
}

fn callable_body(node: Node) -> Option<Node> {
    if let Some(body) = node.child_by_field_name("body") {
        return Some(body);
    }
    statement_block(node)
}

fn async_flag(node: Node, src: &str) -> bool {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .filter(|child| !child.is_named())
        .any(|child| node_text(child, src) == "async");
    found
}

fn contains_await(node: Node) -> bool {
    if node.kind() == "await_expression" {
        return true;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children.into_iter().any(contains_await)
}
