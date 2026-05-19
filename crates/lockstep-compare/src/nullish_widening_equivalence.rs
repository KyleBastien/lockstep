//! Equivalence rule for type-shape nullish widening.
//!
//! Gated on `allow_nullish_widening`: treats `EXPR` on base as equivalent to
//! `EXPR ?? null` or `EXPR ?? undefined` on head, where `EXPR ≡ EXPR` is
//! determined by re-entering the standard walker with a scratch context. This
//! lets widening compose with every other equivalence rule the walker already
//! applies.
//!
//! Gated on `allow_null_undefined_swap` (a sub-flag that only takes effect when
//! `allow_nullish_widening` is also on): additionally accepts a bare `null` ↔
//! `undefined` literal swap at any position. Off by default even when widening
//! is on — `obj.foo === null` and `obj.foo === undefined` are observationally
//! distinct.
//!
//! The rule is directional: only base-narrower / head-widener is allowed.
//! `EXPR ?? null` on base vs bare `EXPR` on head would change the runtime
//! result when `EXPR` evaluates to `undefined`.

use tree_sitter::Node;

use crate::node_utils::{first_named_child, node_text};
use crate::walk::{walk, WalkCtx};

pub(super) fn is_nullish_widening_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if !ctx.allow_nullish_widening {
        return false;
    }
    let base = unwrap_parens(base);
    let head = unwrap_parens(head);
    if ctx.allow_null_undefined_swap
        && is_null_undefined_swap(base, head, ctx.base_src, ctx.head_src)
    {
        return true;
    }
    let Some(lhs) = nullish_widening_lhs(head, ctx.head_src) else {
        return false;
    };
    let scratch_ctx = ctx.scratch();
    let mut scratch_findings = Vec::new();
    walk(&scratch_ctx, base, lhs, &mut scratch_findings);
    scratch_findings.is_empty()
}

fn nullish_widening_lhs<'a>(node: Node<'a>, src: &str) -> Option<Node<'a>> {
    if node.kind() != "binary_expression" {
        return None;
    }
    let operator = node.child_by_field_name("operator")?;
    if operator.kind() != "??" {
        return None;
    }
    let right = unwrap_parens(node.child_by_field_name("right")?);
    if !is_null_or_undefined(right, src) {
        return None;
    }
    Some(unwrap_parens(node.child_by_field_name("left")?))
}

fn is_null_undefined_swap(base: Node, head: Node, base_src: &str, head_src: &str) -> bool {
    (base.kind() == "null" && is_undefined_literal(head, head_src))
        || (head.kind() == "null" && is_undefined_literal(base, base_src))
}

fn is_null_or_undefined(node: Node, src: &str) -> bool {
    node.kind() == "null" || is_undefined_literal(node, src)
}

fn is_undefined_literal(node: Node, src: &str) -> bool {
    if node.kind() == "undefined" {
        return true;
    }
    node.kind() == "identifier" && node_text(node, src) == "undefined"
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
