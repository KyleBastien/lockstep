//! Equivalence rule for array-returning narrowing helpers (Gap 2A).
//!
//! Gated on the `narrowing_helpers_unwrap` config table — a map of helper
//! name → field name on the base shape that the helper unwraps. Without
//! the table the rule is a no-op.
//!
//! Two head patterns are recognized inside a `statement_block`:
//!
//! 1. **Two-statement RAW + narrow shape:**
//!
//!    ```text
//!    const RAW   = SOURCE;
//!    const LOCAL = HELPER(RAW);
//!    ```
//!
//!    Paired against a base `const BASE = SOURCE_EQUIV;` whose source
//!    structurally matches head's `SOURCE`. Registers aliases
//!    `LOCAL` ↔ `BASE.FIELD` and `RAW` ↔ `BASE` for the rest of the block.
//!
//! 2. **One-statement direct shape:**
//!
//!    ```text
//!    const LOCAL = HELPER(SOURCE);
//!    ```
//!
//!    Paired against a base `const BASE = SOURCE_EQUIV;` with the same
//!    structural source match. Registers `LOCAL` ↔ `BASE.FIELD`.
//!
//! Reuses the existing [`HelperCallSiteAlias`] mechanism so leaf-pair
//! resolution (`is_helper_call_site_alias_pair`) handles the substitution
//! without a new leaf rule.

use lockstep_core::Finding;
use tree_sitter::Node;

use crate::node_utils::{compact_node_text, node_text, raw_comparable_children};
use crate::walk::{walk, HelperCallSiteAlias, WalkCtx};

/// Composable block-strip. Scans `head` and `base` for matching
/// unwrap-shape constellations. Returns `true` when at least one alias
/// was registered.
pub(super) fn apply_helper_array_unwrap(child_ctx: &mut WalkCtx, base: Node, head: Node) -> bool {
    if child_ctx.narrowing_helpers_unwrap.is_empty() {
        return false;
    }
    if base.kind() != "statement_block" || head.kind() != "statement_block" {
        return false;
    }
    let head_stmts = named_children(head);
    let base_stmts = named_children(base);
    if head_stmts.is_empty() || base_stmts.is_empty() {
        return false;
    }
    let mut paired_base: Vec<usize> = Vec::new();
    let mut applied = false;
    let mut i = 0;
    while i < head_stmts.len() {
        if let Some(matched) = try_two_stmt(child_ctx, &head_stmts, &base_stmts, &paired_base, i) {
            let base_idx = matched.base_idx;
            apply_two_stmt(child_ctx, &head_stmts, &base_stmts, i, matched);
            paired_base.push(base_idx);
            applied = true;
            i += 2;
            continue;
        }
        if let Some(matched) = try_one_stmt(child_ctx, &head_stmts, &base_stmts, &paired_base, i) {
            let base_idx = matched.base_idx;
            apply_one_stmt(child_ctx, &head_stmts, &base_stmts, i, matched);
            paired_base.push(base_idx);
            applied = true;
            i += 1;
            continue;
        }
        i += 1;
    }
    applied
}

fn named_children(node: Node) -> Vec<Node> {
    raw_comparable_children(node)
        .into_iter()
        .filter(|n| n.is_named())
        .collect()
}

struct TwoStmtMatch<'a> {
    base_idx: usize,
    raw_name: String,
    local_name: String,
    unwrap_field: String,
    base_local: String,
    _source_node: Node<'a>,
}

struct OneStmtMatch<'a> {
    base_idx: usize,
    local_name: String,
    unwrap_field: String,
    base_local: String,
    _source_node: Node<'a>,
}

fn try_two_stmt<'a>(
    ctx: &WalkCtx,
    head_stmts: &[Node<'a>],
    base_stmts: &[Node<'a>],
    paired_base: &[usize],
    i: usize,
) -> Option<TwoStmtMatch<'a>> {
    if i + 1 >= head_stmts.len() {
        return None;
    }
    let (raw_name, source_node) = extract_simple_const(head_stmts[i], ctx.head_src)?;
    let (local_name, helper_name, arg_name) =
        extract_helper_of_identifier(head_stmts[i + 1], ctx.head_src)?;
    if arg_name != raw_name {
        return None;
    }
    let unwrap_field = ctx.narrowing_helpers_unwrap.get(&helper_name)?.clone();
    let (base_idx, base_local) =
        find_matching_base_source(ctx, base_stmts, paired_base, source_node)?;
    Some(TwoStmtMatch {
        base_idx,
        raw_name,
        local_name,
        unwrap_field,
        base_local,
        _source_node: source_node,
    })
}

fn try_one_stmt<'a>(
    ctx: &WalkCtx,
    head_stmts: &[Node<'a>],
    base_stmts: &[Node<'a>],
    paired_base: &[usize],
    i: usize,
) -> Option<OneStmtMatch<'a>> {
    let (local_name, helper_name, source_node) =
        extract_helper_of_expression(head_stmts[i], ctx.head_src)?;
    let unwrap_field = ctx.narrowing_helpers_unwrap.get(&helper_name)?.clone();
    let (base_idx, base_local) =
        find_matching_base_source(ctx, base_stmts, paired_base, source_node)?;
    Some(OneStmtMatch {
        base_idx,
        local_name,
        unwrap_field,
        base_local,
        _source_node: source_node,
    })
}

fn apply_two_stmt(
    child_ctx: &mut WalkCtx,
    head_stmts: &[Node],
    base_stmts: &[Node],
    head_i: usize,
    matched: TwoStmtMatch,
) {
    let unwrapped_path = format!("{}.{}", matched.base_local, matched.unwrap_field);
    child_ctx
        .helper_call_site_aliases
        .push(HelperCallSiteAlias {
            head_local: matched.local_name,
            base_expr_text: unwrapped_path,
        });
    child_ctx
        .helper_call_site_aliases
        .push(HelperCallSiteAlias {
            head_local: matched.raw_name,
            base_expr_text: matched.base_local,
        });
    child_ctx
        .ignored_head_starts
        .push(head_stmts[head_i].start_byte());
    child_ctx
        .ignored_head_starts
        .push(head_stmts[head_i + 1].start_byte());
    child_ctx
        .ignored_base_starts
        .push(base_stmts[matched.base_idx].start_byte());
}

fn apply_one_stmt(
    child_ctx: &mut WalkCtx,
    head_stmts: &[Node],
    base_stmts: &[Node],
    head_i: usize,
    matched: OneStmtMatch,
) {
    let unwrapped_path = format!("{}.{}", matched.base_local, matched.unwrap_field);
    child_ctx
        .helper_call_site_aliases
        .push(HelperCallSiteAlias {
            head_local: matched.local_name,
            base_expr_text: unwrapped_path,
        });
    child_ctx
        .ignored_head_starts
        .push(head_stmts[head_i].start_byte());
    child_ctx
        .ignored_base_starts
        .push(base_stmts[matched.base_idx].start_byte());
}

/// Matches `const NAME = EXPR;` where the value is anything. Returns the
/// declared name and the value node.
fn extract_simple_const<'a>(stmt: Node<'a>, src: &str) -> Option<(String, Node<'a>)> {
    let decl = sole_variable_declarator(stmt)?;
    let name = decl.child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    let value = decl.child_by_field_name("value")?;
    Some((node_text(name, src), value))
}

struct CallDecl<'a> {
    name: String,
    helper: String,
    arg: Node<'a>,
}

/// Matches `const NAME = HELPER(ARG);` — single declarator, identifier
/// name, single-arg call with a bare-identifier callee. Returns the parts
/// without caring about the `ARG` node's kind.
fn parse_helper_call_decl<'a>(stmt: Node<'a>, src: &str) -> Option<CallDecl<'a>> {
    let decl = sole_variable_declarator(stmt)?;
    let name = decl.child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    let value = decl.child_by_field_name("value")?;
    if value.kind() != "call_expression" {
        return None;
    }
    let callee = value.child_by_field_name("function")?;
    if callee.kind() != "identifier" {
        return None;
    }
    let arg = sole_call_argument(value)?;
    Some(CallDecl {
        name: node_text(name, src),
        helper: node_text(callee, src),
        arg,
    })
}

/// Specialization of [`parse_helper_call_decl`] requiring the argument to
/// be a bare identifier. Returns `(NAME, HELPER, IDENT)`.
fn extract_helper_of_identifier(stmt: Node, src: &str) -> Option<(String, String, String)> {
    let decl = parse_helper_call_decl(stmt, src)?;
    if decl.arg.kind() != "identifier" {
        return None;
    }
    Some((decl.name, decl.helper, node_text(decl.arg, src)))
}

/// Specialization of [`parse_helper_call_decl`] rejecting bare-identifier
/// arguments, so the one-stmt rule stays disjoint from the two-stmt rule.
fn extract_helper_of_expression<'a>(
    stmt: Node<'a>,
    src: &str,
) -> Option<(String, String, Node<'a>)> {
    let decl = parse_helper_call_decl(stmt, src)?;
    if decl.arg.kind() == "identifier" {
        return None;
    }
    Some((decl.name, decl.helper, decl.arg))
}

fn find_matching_base_source(
    ctx: &WalkCtx,
    base_stmts: &[Node],
    paired: &[usize],
    head_source: Node,
) -> Option<(usize, String)> {
    for (idx, stmt) in base_stmts.iter().enumerate() {
        if paired.contains(&idx) {
            continue;
        }
        let Some((base_name, base_source)) = extract_simple_const(*stmt, ctx.base_src) else {
            continue;
        };
        if sources_equivalent(ctx, base_source, head_source) {
            return Some((idx, base_name));
        }
    }
    None
}

fn sources_equivalent(ctx: &WalkCtx, base_src_node: Node, head_src_node: Node) -> bool {
    if compact_node_text(base_src_node, ctx.base_src)
        == compact_node_text(head_src_node, ctx.head_src)
    {
        return true;
    }
    let scratch = ctx.scratch();
    let mut findings: Vec<Finding> = Vec::new();
    walk(&scratch, base_src_node, head_src_node, &mut findings);
    findings.is_empty()
}

fn sole_variable_declarator(stmt: Node) -> Option<Node> {
    if !matches!(stmt.kind(), "lexical_declaration" | "variable_declaration") {
        return None;
    }
    let declarators: Vec<Node> = raw_comparable_children(stmt)
        .into_iter()
        .filter(|c| c.kind() == "variable_declarator")
        .collect();
    if declarators.len() != 1 {
        return None;
    }
    Some(declarators[0])
}

fn sole_call_argument(call: Node) -> Option<Node> {
    let arguments = call.child_by_field_name("arguments")?;
    let args: Vec<Node> = raw_comparable_children(arguments)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    if args.len() != 1 {
        return None;
    }
    Some(args[0])
}
