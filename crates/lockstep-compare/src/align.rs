use std::collections::HashSet;

use tree_sitter::Node;

use crate::node_utils::{compact_node_text, find_descendant, first_named_child, node_text};
use crate::walk::WalkCtx;

pub(super) struct Alignment {
    pub(super) pairs: Vec<(usize, usize)>,
    pub(super) unmatched_base: Vec<usize>,
    pub(super) unmatched_head: Vec<usize>,
}

pub(super) fn align_children(ctx: &WalkCtx, base: &[Node], head: &[Node]) -> Alignment {
    let mut dp = vec![vec![0usize; head.len() + 1]; base.len() + 1];
    for i in (0..base.len()).rev() {
        for j in (0..head.len()).rev() {
            dp[i][j] = if nodes_align(ctx, base[i], head[j]) {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut pairs = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;
    while i < base.len() && j < head.len() {
        if nodes_align(ctx, base[i], head[j]) {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }

    let matched_base = pairs.iter().map(|(idx, _)| *idx).collect::<HashSet<_>>();
    let matched_head = pairs.iter().map(|(_, idx)| *idx).collect::<HashSet<_>>();
    let unmatched_base = (0..base.len())
        .filter(|idx| !matched_base.contains(idx))
        .collect();
    let unmatched_head = (0..head.len())
        .filter(|idx| !matched_head.contains(idx))
        .collect();
    Alignment {
        pairs,
        unmatched_base,
        unmatched_head,
    }
}

fn nodes_align(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if base.kind() != head.kind() {
        return false;
    }
    match (
        stable_anchor(base, ctx.base_src),
        stable_anchor(head, ctx.head_src),
    ) {
        (Some(base_anchor), Some(head_anchor)) => base_anchor == head_anchor,
        (None, None) => true,
        _ => false,
    }
}

fn stable_anchor(node: Node, src: &str) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        return Some(format!("name:{}", node_text(name, src)));
    }
    match node.kind() {
        "lexical_declaration" | "variable_declaration" => declaration_anchor(node, src),
        "expression_statement" => expression_statement_anchor(node, src),
        "return_statement" => Some("return".into()),
        "if_statement" => Some("if".into()),
        _ => None,
    }
}

fn declaration_anchor(node: Node, src: &str) -> Option<String> {
    find_descendant(node, "variable_declarator").and_then(|decl| {
        decl.child_by_field_name("name")
            .map(|name| format!("var:{}", node_text(name, src)))
    })
}

fn expression_statement_anchor(node: Node, src: &str) -> Option<String> {
    let expression = first_named_child(node)?;
    if expression.kind() == "assignment_expression" {
        return expression
            .child_by_field_name("left")
            .map(|left| format!("assign:{}", compact_node_text(left, src)));
    }
    expression_anchor(expression, src).map(|anchor| format!("expr:{anchor}"))
}

fn expression_anchor(node: Node, src: &str) -> Option<String> {
    match node.kind() {
        "call_expression" => node
            .child_by_field_name("function")
            .map(|function| compact_node_text(function, src)),
        "member_expression" => node
            .child_by_field_name("property")
            .map(|property| node_text(property, src)),
        "identifier" | "property_identifier" => Some(node_text(node, src)),
        _ => first_named_child(node).and_then(|child| expression_anchor(child, src)),
    }
}
