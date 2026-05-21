//! Equivalence rule for `destructure-then-narrow`.
//!
//! Gated on `allow_destructure_then_narrow` (typically cascaded from
//! `allow_pure_narrowing_helper`). Composes the narrowing-helper allowlist
//! with destructure renames.
//!
//! Head shape:
//!
//! ```text
//! const { K1: RAW1, K2: RAW2, ... } = SRC;
//! const K1 = HELPER(RAW1) ?? DEFAULT;
//! const K2 = HELPER(RAW2) ?? DEFAULT;
//! ```
//!
//! Base shape:
//!
//! ```text
//! const { K1, K2, ... } = SRC;
//! ```
//!
//! When source expressions agree and the same key set appears on both
//! sides, all `K_i` reads downstream on both sides reference identifiers
//! of the same name — so the equivalence reduces to "strip both
//! destructures + all narrow stmts on head, walk the rest in lockstep."
//!
//! Out of scope (v1):
//! - Mixed shorthand on head (`{ K1, K2: RAW2 }`).
//! - Nested object/array patterns inside the destructure.
//! - Rest elements (`...rest`).
//! - Default values in the destructure pattern.

use tree_sitter::Node;

use crate::helper_call_site_substitution::{extract_helper_call_site, Extract};
use crate::node_utils::{compact_node_text, node_text, raw_comparable_children};
use crate::walk::{walk, WalkCtx};

/// Composable block-strip. Returns `true` when at least one destructure +
/// narrow group on head was paired with a matching destructure on base and
/// both groups were marked ignored.
pub(super) fn apply_destructure_then_narrow(
    child_ctx: &mut WalkCtx,
    base: Node,
    head: Node,
) -> bool {
    if !child_ctx.allow_destructure_then_narrow {
        return false;
    }
    if child_ctx.recognized_narrowing_helpers.is_empty() {
        return false;
    }
    if base.kind() != "statement_block" || head.kind() != "statement_block" {
        return false;
    }
    let base_stmts = named_children(base);
    let head_stmts = named_children(head);
    if base_stmts.is_empty() || head_stmts.is_empty() {
        return false;
    }
    pair_groups(child_ctx, &base_stmts, &head_stmts)
}

fn named_children(node: Node) -> Vec<Node> {
    raw_comparable_children(node)
        .into_iter()
        .filter(|n| n.is_named())
        .collect()
}

fn pair_groups(child_ctx: &mut WalkCtx, base_stmts: &[Node], head_stmts: &[Node]) -> bool {
    let mut paired_base: Vec<usize> = Vec::new();
    let mut applied = false;
    let mut i = 0;
    while i < head_stmts.len() {
        let Some(group) = head_group_at(child_ctx, head_stmts, i) else {
            i += 1;
            continue;
        };
        let Some(base_idx) = find_matching_base(child_ctx, base_stmts, &paired_base, &group) else {
            i += group.consumed;
            continue;
        };
        ignore_group_bytes(
            child_ctx,
            base_stmts[base_idx],
            head_stmts,
            i,
            group.consumed,
        );
        paired_base.push(base_idx);
        applied = true;
        i += group.consumed;
    }
    applied
}

fn ignore_group_bytes(
    child_ctx: &mut WalkCtx,
    base_stmt: Node,
    head_stmts: &[Node],
    head_start: usize,
    consumed: usize,
) {
    child_ctx.ignored_base_starts.push(base_stmt.start_byte());
    for k in 0..consumed {
        child_ctx
            .ignored_head_starts
            .push(head_stmts[head_start + k].start_byte());
    }
}

struct HeadGroup<'a> {
    consumed: usize,
    source_node: Node<'a>,
    keys: Vec<String>,
}

/// Recognizes the head destructure + N narrow stmts starting at `i`.
/// `consumed` is `1 + N` (the destructure plus each narrow stmt).
fn head_group_at<'a>(ctx: &WalkCtx, stmts: &[Node<'a>], i: usize) -> Option<HeadGroup<'a>> {
    let destruct = parse_head_destructure(stmts[i], ctx.head_src)?;
    if destruct.bindings.is_empty() {
        return None;
    }
    let n = destruct.bindings.len();
    if i + n >= stmts.len() {
        return None;
    }
    let mut keys: Vec<String> = Vec::with_capacity(n);
    for binding in &destruct.bindings {
        let narrow_stmt = stmts[i + 1 + keys.len()];
        let extract = extract_helper_call_site(narrow_stmt, ctx)?;
        if !narrow_matches_binding(&extract, binding) {
            return None;
        }
        keys.push(binding.key.clone());
    }
    Some(HeadGroup {
        consumed: 1 + n,
        source_node: destruct.source,
        keys,
    })
}

fn narrow_matches_binding(extract: &Extract, binding: &HeadBinding) -> bool {
    extract.local_name == binding.key && extract.base_expr_text == binding.head_local
}

struct HeadDestructure<'a> {
    source: Node<'a>,
    bindings: Vec<HeadBinding>,
}

struct HeadBinding {
    key: String,
    head_local: String,
}

fn parse_head_destructure<'a>(stmt: Node<'a>, src: &str) -> Option<HeadDestructure<'a>> {
    let decl = sole_declarator(stmt)?;
    let name = decl.child_by_field_name("name")?;
    if name.kind() != "object_pattern" {
        return None;
    }
    let mut bindings = Vec::new();
    for child in raw_comparable_children(name) {
        let binding = read_head_pair_binding(child, src)?;
        bindings.push(binding);
    }
    let source = decl.child_by_field_name("value")?;
    Some(HeadDestructure { source, bindings })
}

fn read_head_pair_binding(node: Node, src: &str) -> Option<HeadBinding> {
    if node.kind() != "pair_pattern" {
        return None;
    }
    let key = node.child_by_field_name("key")?;
    let value = node.child_by_field_name("value")?;
    if value.kind() != "identifier" {
        return None;
    }
    Some(HeadBinding {
        key: node_text(key, src),
        head_local: node_text(value, src),
    })
}

fn find_matching_base(
    ctx: &WalkCtx,
    base_stmts: &[Node],
    paired: &[usize],
    head_group: &HeadGroup,
) -> Option<usize> {
    for (idx, stmt) in base_stmts.iter().enumerate() {
        if paired.contains(&idx) {
            continue;
        }
        let Some(base_destruct) = parse_base_destructure(*stmt, ctx.base_src) else {
            continue;
        };
        if !key_sets_match(&base_destruct.keys, &head_group.keys) {
            continue;
        }
        if !sources_equivalent(ctx, base_destruct.source, head_group.source_node) {
            continue;
        }
        return Some(idx);
    }
    None
}

struct BaseDestructure<'a> {
    source: Node<'a>,
    keys: Vec<String>,
}

fn parse_base_destructure<'a>(stmt: Node<'a>, src: &str) -> Option<BaseDestructure<'a>> {
    let decl = sole_declarator(stmt)?;
    let name = decl.child_by_field_name("name")?;
    if name.kind() != "object_pattern" {
        return None;
    }
    let mut keys = Vec::new();
    for child in raw_comparable_children(name) {
        keys.push(base_pattern_key(child, src)?);
    }
    let source = decl.child_by_field_name("value")?;
    Some(BaseDestructure { source, keys })
}

fn base_pattern_key(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "shorthand_property_identifier_pattern" => Some(node_text(node, src)),
        "pair_pattern" => {
            let key = node.child_by_field_name("key")?;
            Some(node_text(key, src))
        }
        _ => None,
    }
}

fn key_sets_match(base_keys: &[String], head_keys: &[String]) -> bool {
    if base_keys.len() != head_keys.len() {
        return false;
    }
    let mut base_sorted = base_keys.to_vec();
    base_sorted.sort();
    let mut head_sorted = head_keys.to_vec();
    head_sorted.sort();
    base_sorted == head_sorted
}

fn sources_equivalent(ctx: &WalkCtx, base_src_node: Node, head_src_node: Node) -> bool {
    if compact_node_text(base_src_node, ctx.base_src)
        == compact_node_text(head_src_node, ctx.head_src)
    {
        return true;
    }
    let scratch = ctx.scratch();
    let mut findings = Vec::new();
    walk(&scratch, base_src_node, head_src_node, &mut findings);
    findings.is_empty()
}

fn sole_declarator(stmt: Node) -> Option<Node> {
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
