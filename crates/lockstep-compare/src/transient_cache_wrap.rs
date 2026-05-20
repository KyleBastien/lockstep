//! Equivalence rule for the "use a fresh local to feed the cache" pattern.
//!
//! Gated on `allow_transient_cache_wrap`. Treats this two-statement base
//! shape
//!
//! ```text
//! CACHE = X;
//! CACHE = unwrap(CACHE);
//! ```
//!
//! as equivalent to the head shape
//!
//! ```text
//! const LOCAL = X;
//! CACHE = unwrap(LOCAL);
//! ```
//!
//! where `LOCAL` is a fresh head identifier used only inside `unwrap(...)`.
//!
//! TS forces this when the cache field is narrowly typed (`Foo | null`) and
//! the raw response (`{data: Foo[]}`) cannot transiently sit in it without
//! a cast. The rewrite is asymmetric — never accept the reverse direction.
//!
//! Composes with `allow_closure_cache_field_alias` (so `CACHE` on base may
//! be a bare identifier and on head a `this._cache` member) and with
//! `allow_array_first_element_or_null` (the typical `unwrap`).

use tree_sitter::Node;

use crate::node_utils::{first_named_child, node_text, raw_comparable_children};
use crate::walk::{walk, TransientLocal, WalkCtx};

/// Returns `true` when an in-context transient-local alias matches the
/// identifier pair.
pub(super) fn is_transient_local_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if base.kind() != "identifier" || head.kind() != "identifier" {
        return false;
    }
    let base_text = node_text(base, ctx.base_src);
    let head_text = node_text(head, ctx.head_src);
    ctx.transient_locals
        .iter()
        .any(|t| t.base_name == base_text && t.head_name == head_text)
}

/// Composable variant: detects the transient-cache-wrap pattern on the given
/// block pair and, if matched, pushes its ignored byte ranges + alias onto
/// `child_ctx`. Caller is responsible for running `walk_regular` afterwards.
/// Returns `true` when the pattern matched.
pub(super) fn apply_transient_cache_wrap(child_ctx: &mut WalkCtx, base: Node, head: Node) -> bool {
    if !child_ctx.allow_transient_cache_wrap {
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
    if base_stmts.len() != head_stmts.len() {
        return false;
    }
    let Some(found) = find_transient_pair(child_ctx, &base_stmts, &head_stmts) else {
        return false;
    };
    child_ctx
        .ignored_base_starts
        .push(found.base_stmt1.start_byte());
    child_ctx
        .ignored_head_starts
        .push(found.head_stmt1.start_byte());
    child_ctx.transient_locals.push(TransientLocal {
        head_name: found.local_name,
        base_name: found.cache_name,
    });
    true
}

struct TransientMatch<'a> {
    base_stmt1: Node<'a>,
    head_stmt1: Node<'a>,
    cache_name: String,
    local_name: String,
}

fn find_transient_pair<'a>(
    ctx: &WalkCtx,
    base_stmts: &[Node<'a>],
    head_stmts: &[Node<'a>],
) -> Option<TransientMatch<'a>> {
    for i in 0..base_stmts.len().saturating_sub(1) {
        let base_pair = match base_pair_shape(ctx, base_stmts[i], base_stmts[i + 1]) {
            Some(p) => p,
            None => continue,
        };
        let head_pair = match head_pair_shape(ctx, head_stmts[i], head_stmts[i + 1]) {
            Some(p) => p,
            None => continue,
        };
        if !value_exprs_equivalent(ctx, base_pair.value, head_pair.value) {
            continue;
        }
        if !local_is_fresh(ctx, head_stmts, i, &head_pair.local_name) {
            continue;
        }
        return Some(TransientMatch {
            base_stmt1: base_stmts[i],
            head_stmt1: head_stmts[i],
            cache_name: base_pair.cache_name,
            local_name: head_pair.local_name,
        });
    }
    None
}

struct BasePair<'a> {
    cache_name: String,
    value: Node<'a>,
}

struct HeadPair<'a> {
    local_name: String,
    value: Node<'a>,
}

/// Base shape: `expr_stmt(CACHE = X);  expr_stmt(CACHE = ...uses CACHE...);`
fn base_pair_shape<'a>(ctx: &WalkCtx, stmt1: Node<'a>, stmt2: Node<'a>) -> Option<BasePair<'a>> {
    let (lhs1, rhs1) = assignment_parts(stmt1)?;
    if lhs1.kind() != "identifier" {
        return None;
    }
    let cache_name = node_text(lhs1, ctx.base_src);
    let (lhs2, rhs2) = assignment_parts(stmt2)?;
    if lhs2.kind() != "identifier" {
        return None;
    }
    if node_text(lhs2, ctx.base_src) != cache_name {
        return None;
    }
    if !identifier_referenced(rhs2, ctx.base_src, &cache_name) {
        return None;
    }
    Some(BasePair {
        cache_name,
        value: rhs1,
    })
}

/// Head shape: `const LOCAL = X;  expr_stmt(this.PROP = ...uses LOCAL...);`
fn head_pair_shape<'a>(ctx: &WalkCtx, stmt1: Node<'a>, stmt2: Node<'a>) -> Option<HeadPair<'a>> {
    if stmt1.kind() != "lexical_declaration" && stmt1.kind() != "variable_declaration" {
        return None;
    }
    let declarators: Vec<Node> = raw_comparable_children(stmt1)
        .into_iter()
        .filter(|c| c.kind() == "variable_declarator")
        .collect();
    if declarators.len() != 1 {
        return None;
    }
    let declarator = declarators[0];
    let name = declarator.child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    let value = declarator.child_by_field_name("value")?;
    let local_name = node_text(name, ctx.head_src);

    let (lhs2, rhs2) = assignment_parts(stmt2)?;
    let _ = lhs2;
    if !identifier_referenced(rhs2, ctx.head_src, &local_name) {
        return None;
    }
    Some(HeadPair { local_name, value })
}

fn assignment_parts<'a>(stmt: Node<'a>) -> Option<(Node<'a>, Node<'a>)> {
    if stmt.kind() != "expression_statement" {
        return None;
    }
    let expr = first_named_child(stmt)?;
    if expr.kind() != "assignment_expression" {
        return None;
    }
    let left = expr.child_by_field_name("left")?;
    let right = expr.child_by_field_name("right")?;
    Some((left, right))
}

fn identifier_referenced(node: Node, src: &str, name: &str) -> bool {
    if node.kind() == "identifier" && node_text(node, src) == name {
        return true;
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .any(|child| identifier_referenced(child, src, name))
}

fn local_is_fresh(ctx: &WalkCtx, head_stmts: &[Node], wrap_idx: usize, local_name: &str) -> bool {
    for (i, stmt) in head_stmts.iter().enumerate() {
        if i == wrap_idx || i == wrap_idx + 1 {
            continue;
        }
        if identifier_referenced(*stmt, ctx.head_src, local_name) {
            return false;
        }
    }
    true
}

fn value_exprs_equivalent(ctx: &WalkCtx, base_value: Node, head_value: Node) -> bool {
    let scratch_ctx = ctx.scratch();
    let mut scratch = Vec::new();
    walk(&scratch_ctx, base_value, head_value, &mut scratch);
    scratch.is_empty()
}
