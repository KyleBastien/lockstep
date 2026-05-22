//! Equivalence rule for the composition of a registered zero-arg helper
//! alias + an optional-chain removal + a `??` widening with a safe-default
//! literal at one AST position.
//!
//! Gated on `allow_alias_helper_optional_chain_composition`. Reuses
//! [`crate::helper_zero_arg_alias::is_helper_zero_arg_alias_pair`] to
//! resolve the alias + optional-chain substitution and
//! [`crate::pure_narrowing_helper::is_safe_default`] to recognize the
//! literal default.
//!
//! Head shape (at any AST position):
//!
//! ```text
//! LOCAL.PROP ?? DEFAULT
//! ```
//!
//! where `LOCAL` was previously declared in head as `const LOCAL = HELPER();`
//! and `HELPER` is registered in `narrowing_helpers_aliases` mapping to
//! some `BASE_PATH` (e.g. `config.cdn_config?` or `config.cdn_config`).
//! Both forms are accepted: the alias resolver tries the literal
//! substitution first and, if it fails, retries with `?` inserted at the
//! alias/property-accessor boundary. The base node at the same position
//! has compact text equal to either `BASE_PATH.PROP` or `BASE_PATH?.PROP`.
//!
//! `DEFAULT` is one of `string` | `number` | `null` | `true` | `false` |
//! `undefined` | empty-object | empty-array, per
//! [`crate::pure_narrowing_helper::is_safe_default`].
//!
//! **WARNING — observable behavior change at the edge.** When `BASE_PATH`
//! is actually nullish at runtime, base interpolates the string
//! `"undefined"` (JS string coercion of an optional-chain miss), head
//! substitutes the literal `DEFAULT`. The rule defaults OFF.

use tree_sitter::Node;

use crate::helper_zero_arg_alias::is_helper_zero_arg_alias_pair;
use crate::node_utils::{first_named_child, node_text};
use crate::pure_narrowing_helper::is_safe_default;
use crate::walk::WalkCtx;

/// Leaf-pair pre-empt: head is `LHS ?? SAFE_DEFAULT` and `LHS` resolves
/// against `base` via the zero-arg alias rule.
pub(super) fn is_alias_helper_widening_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if !ctx.allow_alias_helper_optional_chain_composition {
        return false;
    }
    if ctx.helper_zero_arg_aliases.is_empty() {
        return false;
    }
    let head = unwrap_parens(head);
    let Some(lhs) = strip_widening_with_safe_default(head, ctx.head_src) else {
        return false;
    };
    is_helper_zero_arg_alias_pair(ctx, base, lhs)
}

fn strip_widening_with_safe_default<'a>(node: Node<'a>, src: &str) -> Option<Node<'a>> {
    if node.kind() != "binary_expression" {
        return None;
    }
    let op = node.child_by_field_name("operator")?;
    if node_text(op, src) != "??" {
        return None;
    }
    let right = unwrap_parens(node.child_by_field_name("right")?);
    if !is_safe_default(right, src) {
        return None;
    }
    Some(unwrap_parens(node.child_by_field_name("left")?))
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
