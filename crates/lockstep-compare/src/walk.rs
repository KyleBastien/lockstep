//! Dual-walk AST comparator.
//!
//! Both inputs are parsed with `tree-sitter-javascript` in `entry::compare`.
//! Walks named children in lockstep, comparing `kind()`, child arity, and
//! (at leaves) canonical token text. Skips `comment` nodes and other trivia.

use lockstep_core::Finding;
use tree_sitter::Node;

use crate::align::align_children;
use crate::array_first_equivalence::is_array_first_pair;
use crate::async_propagation::{maybe_unwrap_await, try_callable_async_propagation};
use crate::class_equivalence::{is_cache_alias_pair, walk_class_body};
use crate::defensive_log_guard::maybe_unwrap_log_guard;
use crate::defensive_null_guard::apply_defensive_null_guard;
use crate::destructure_then_narrow::apply_destructure_then_narrow;
use crate::findings::{arity_mismatch, kind_mismatch, token_mismatch, unmatched_child};
use crate::handler_arrow_expansion::maybe_unwrap_handler_arrow_expansion;
use crate::helper_call_site_substitution::{
    apply_helper_call_site_substitution, is_helper_call_site_alias_pair,
};
use crate::node_utils::{is_meaningful_unnamed, is_trivia, raw_comparable_children};
use crate::non_null_alias_local::{apply_non_null_alias_local, is_non_null_alias_pair};
use crate::nullish_widening_equivalence::is_nullish_widening_pair;
use crate::optional_chain::handle_optional_chain;
use crate::promise_settled_discrimination::apply_promise_settled_discrimination;
use crate::pure_narrowing_helper::is_pure_narrowing_helper_pair;
use crate::request_field_narrowing::{
    apply_request_field_narrowing, is_narrowed_request_field_pair,
};
use crate::tokens::canonical;
use crate::transient_cache_wrap::{apply_transient_cache_wrap, is_transient_local_pair};
use crate::unknown_catch_narrowing::{
    apply_unknown_catch_narrowing, is_catch_narrowed_pair, is_unknown_catch_narrowing_pair,
};

pub use crate::entry::compare;
pub(super) use crate::walk_ctx::{
    CacheAlias, CatchNarrowedLocal, HelperCallSiteAlias, NarrowedRequestField, NonNullAliasLocal,
    Side, TransientLocal, WalkCtx,
};

pub(super) fn walk(ctx: &WalkCtx, base: Node, head: Node, findings: &mut Vec<Finding>) {
    if !ctx.report_all && !findings.is_empty() {
        return;
    }
    if leaf_alias_consumed(ctx, base, head) {
        return;
    }
    if block_rule_consumed(ctx, base, head, findings) {
        return;
    }
    if base.kind() != head.kind() {
        findings.push(kind_mismatch(ctx, base, head));
        return;
    }
    if ctx.allow_constructor_assigned_method_equivalence
        && base.kind() == "class_body"
        && walk_class_body(ctx, base, head, findings)
    {
        return;
    }
    walk_regular(ctx, base, head, findings);
}

/// Leaf-level pre-empts that resolve a pair via a registered alias or a
/// purely structural equivalence — no body walk required.
fn leaf_alias_consumed(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    is_cache_alias_pair(ctx, base, head)
        || is_transient_local_pair(ctx, base, head)
        || is_narrowed_request_field_pair(ctx, base, head)
        || is_non_null_alias_pair(ctx, base, head)
        || is_catch_narrowed_pair(ctx, base, head)
        || is_unknown_catch_narrowing_pair(ctx, base, head)
        || is_helper_call_site_alias_pair(ctx, base, head)
        || is_pure_narrowing_helper_pair(ctx, base, head)
        || is_array_first_pair(ctx, base, head)
        || is_nullish_widening_pair(ctx, base, head)
}

/// Block- and callable-scoped pre-empts that may consume the pair by running
/// their own sub-walk and feeding findings back to the outer pass.
fn block_rule_consumed(ctx: &WalkCtx, base: Node, head: Node, findings: &mut Vec<Finding>) -> bool {
    // Non-composable pre-empts that operate on non-block pairs or set their
    // own context for descendant walks.
    if maybe_unwrap_await(ctx, base, head, findings) {
        return true;
    }
    if maybe_unwrap_log_guard(ctx, base, head, findings) {
        return true;
    }
    if maybe_unwrap_handler_arrow_expansion(ctx, base, head, findings) {
        return true;
    }
    if is_method_definition_pair(base, head)
        && try_callable_async_propagation(ctx, base, head, findings)
    {
        return true;
    }

    // Composable block-strip rules. Each may push ignored byte ranges and/or
    // register scoped aliases onto a shared child_ctx; none calls
    // walk_regular itself.
    if base.kind() != "statement_block" || head.kind() != "statement_block" {
        return false;
    }
    let mut child_ctx = ctx.clone();
    let mut applied = false;
    applied |= apply_transient_cache_wrap(&mut child_ctx, base, head);
    applied |= apply_request_field_narrowing(&mut child_ctx, base, head);
    applied |= apply_non_null_alias_local(&mut child_ctx, base, head);
    applied |= apply_unknown_catch_narrowing(&mut child_ctx, base, head);
    applied |= apply_promise_settled_discrimination(&mut child_ctx, base, head);
    applied |= apply_defensive_null_guard(&mut child_ctx, base, head);
    applied |= apply_helper_call_site_substitution(&mut child_ctx, base, head);
    applied |= apply_destructure_then_narrow(&mut child_ctx, base, head);
    if applied {
        walk_regular(&child_ctx, base, head, findings);
        return true;
    }
    false
}

pub(super) fn walk_regular(ctx: &WalkCtx, base: Node, head: Node, findings: &mut Vec<Finding>) {
    if is_atomic(base.kind()) {
        compare_leaf(ctx, base, head, findings);
        return;
    }
    if handle_optional_chain(ctx, base, head, findings) {
        return;
    }
    let base_children = comparable_children(ctx, base, Side::Base);
    let head_children = comparable_children(ctx, head, Side::Head);
    walk_collected(
        ctx,
        NodePair { base, head },
        ChildPair {
            base: base_children,
            head: head_children,
        },
        findings,
    );
}

pub(super) fn walk_optional_chain_more_defensive(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    findings: &mut Vec<Finding>,
) {
    let base_children = comparable_children(ctx, base, Side::Base);
    let head_children: Vec<Node> = comparable_children(ctx, head, Side::Head)
        .into_iter()
        .filter(|n| n.kind() != "optional_chain")
        .collect();
    walk_collected(
        ctx,
        NodePair { base, head },
        ChildPair {
            base: base_children,
            head: head_children,
        },
        findings,
    );
}

/// Mirror of [`walk_optional_chain_more_defensive`] used when the head has
/// *removed* a base `?.` and the dead-defensive-optional-chain rule has
/// accepted the removal as equivalent. Filters the `optional_chain` child
/// from the base side so the remaining object/property children walk in
/// lockstep with the head's regular member access.
pub(super) fn walk_optional_chain_less_defensive(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    findings: &mut Vec<Finding>,
) {
    let base_children: Vec<Node> = comparable_children(ctx, base, Side::Base)
        .into_iter()
        .filter(|n| n.kind() != "optional_chain")
        .collect();
    let head_children = comparable_children(ctx, head, Side::Head);
    walk_collected(
        ctx,
        NodePair { base, head },
        ChildPair {
            base: base_children,
            head: head_children,
        },
        findings,
    );
}

pub(super) struct NodePair<'a> {
    pub(super) base: Node<'a>,
    pub(super) head: Node<'a>,
}

struct ChildPair<'a> {
    base: Vec<Node<'a>>,
    head: Vec<Node<'a>>,
}

fn walk_collected(
    ctx: &WalkCtx,
    nodes: NodePair,
    children: ChildPair,
    findings: &mut Vec<Finding>,
) {
    let NodePair { base, head } = nodes;
    let ChildPair {
        base: base_children,
        head: head_children,
    } = children;
    if base_children.len() != head_children.len() {
        if !walk_aligned_arity_mismatch(
            ctx,
            (base, head),
            (&base_children, &head_children),
            findings,
        ) {
            findings.push(arity_mismatch(
                ctx,
                base,
                head,
                base_children.len(),
                head_children.len(),
            ));
        }
        return;
    }
    if base_children.is_empty() {
        if has_filtered_children(ctx, base, Side::Base)
            || has_filtered_children(ctx, head, Side::Head)
        {
            return;
        }
        compare_leaf(ctx, base, head, findings);
        return;
    }
    walk_children(ctx, base_children, head_children, findings);
}

fn walk_children(
    ctx: &WalkCtx,
    base_children: Vec<Node>,
    head_children: Vec<Node>,
    findings: &mut Vec<Finding>,
) {
    for (b, h) in base_children.into_iter().zip(head_children) {
        walk(ctx, b, h, findings);
        if !ctx.report_all && !findings.is_empty() {
            return;
        }
    }
}

fn walk_aligned_arity_mismatch(
    ctx: &WalkCtx,
    parents: (Node, Node),
    children: (&[Node], &[Node]),
    findings: &mut Vec<Finding>,
) -> bool {
    let (base, head) = parents;
    let (base_children, head_children) = children;
    let alignment = align_children(ctx, base_children, head_children);
    if alignment.pairs.is_empty() {
        return false;
    }
    for idx in alignment.unmatched_base {
        findings.push(unmatched_child(
            ctx,
            base,
            head,
            base_children[idx],
            Side::Base,
        ));
        if !ctx.report_all {
            return true;
        }
    }
    for idx in alignment.unmatched_head {
        findings.push(unmatched_child(
            ctx,
            base,
            head,
            head_children[idx],
            Side::Head,
        ));
        if !ctx.report_all {
            return true;
        }
    }
    for (base_idx, head_idx) in alignment.pairs {
        walk(
            ctx,
            base_children[base_idx],
            head_children[head_idx],
            findings,
        );
        if !ctx.report_all && !findings.is_empty() {
            return true;
        }
    }
    true
}

fn compare_leaf(ctx: &WalkCtx, base: Node, head: Node, findings: &mut Vec<Finding>) {
    let base_text = base.utf8_text(ctx.base_src.as_bytes()).unwrap_or("");
    let head_text = head.utf8_text(ctx.head_src.as_bytes()).unwrap_or("");
    if canonical(base.kind(), base_text) != canonical(head.kind(), head_text) {
        findings.push(token_mismatch(ctx, base, head, base_text, head_text));
    }
}

/// Children that participate in the structural compare.
///
/// Includes every named child that isn't trivia, plus unnamed children whose
/// kind is *meaningful* — operators, keyword operators (`typeof`, `instanceof`,
/// `in`, `of`, `void`, `delete`). Excludes pure punctuation and the declaration
/// keywords (`let`/`const`/`var`) so `var` → `const`/`let` normalization is
/// silently accepted.
fn comparable_children<'a>(ctx: &WalkCtx, node: Node<'a>, side: Side) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if is_trivia(child) {
            continue;
        }
        if is_ignored(ctx, child, side) {
            continue;
        }
        if child.is_named() || is_meaningful_unnamed(child.kind()) {
            out.push(child);
        }
    }
    out
}

fn is_ignored(ctx: &WalkCtx, node: Node, side: Side) -> bool {
    let starts = match side {
        Side::Base => &ctx.ignored_base_starts,
        Side::Head => &ctx.ignored_head_starts,
    };
    starts.contains(&node.start_byte())
}

fn has_filtered_children(ctx: &WalkCtx, node: Node, side: Side) -> bool {
    raw_comparable_children(node)
        .into_iter()
        .any(|child| is_ignored(ctx, child, side))
}

fn is_method_definition_pair(base: Node, head: Node) -> bool {
    base.kind() == "method_definition" && head.kind() == "method_definition"
}

fn is_atomic(kind: &str) -> bool {
    matches!(
        kind,
        "string" | "regex" | "number" | "identifier" | "property_identifier"
    )
}
