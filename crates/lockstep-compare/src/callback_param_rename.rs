//! Equivalence rule for scope-bounded callback parameter rename.
//!
//! When comparing two callbacks (arrow functions or function expressions)
//! whose single bare-identifier parameter has been renamed — e.g. head
//! `(admin) => admin.email` vs base `(client) => client.email` — accept
//! the rename for the scope of the callback body. Composes with any alias
//! rule that already accepts the differing receivers (e.g. helper-derived
//! aliases that map `adminRows` ↔ `clientAdmins.data`).
//!
//! Safety: tree-sitter doesn't carry a scope resolver, so we conservatively
//! reject renames whose target name appears anywhere in the other side's
//! callback body. That prevents the worst confusion (closure-name
//! collision where the renamed identifier was already bound to a different
//! value at the closing scope), while accepting the common
//! `map`/`filter`/`forEach` style callback that only references its
//! parameter.
//!
//! Out of scope (v1):
//! - Multi-parameter callbacks (`(idx, item) => …`). The shadowing check
//!   would need to be per-parameter.
//! - Destructured parameters (`({ id }) => id`).
//! - Default parameters / rest elements.

use tree_sitter::Node;

use crate::node_utils::{node_text, raw_comparable_children};
use crate::walk::{walk_regular, ParamRename, WalkCtx};
use lockstep_core::Finding;

/// Returns `true` after handling the entire pair via `walk_regular` with a
/// scoped param-rename alias. Caller must skip its own walk in that case.
/// Returns `false` when the pair isn't a renameable callback pair.
pub(super) fn maybe_apply_param_rename(
    ctx: &WalkCtx,
    base: Node,
    head: Node,
    findings: &mut Vec<Finding>,
) -> bool {
    if !is_callback_kind_pair(base, head) {
        return false;
    }
    let Some(rename) = detect_single_param_rename(ctx, base, head) else {
        return false;
    };
    if shadowing_risk(ctx, base, head, &rename) {
        return false;
    }
    let mut child_ctx = ctx.clone();
    child_ctx.param_renames.push(rename);
    walk_regular(&child_ctx, base, head, findings);
    true
}

/// Leaf-pair pre-empt: identifier ↔ identifier resolved through an active
/// `param_renames` mapping in either direction.
pub(super) fn is_param_rename_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if ctx.param_renames.is_empty() {
        return false;
    }
    if base.kind() != "identifier" || head.kind() != "identifier" {
        return false;
    }
    let base_text = node_text(base, ctx.base_src);
    let head_text = node_text(head, ctx.head_src);
    ctx.param_renames
        .iter()
        .any(|r| r.head_name == head_text && r.base_name == base_text)
}

fn is_callback_kind_pair(base: Node, head: Node) -> bool {
    if base.kind() != head.kind() {
        return false;
    }
    matches!(
        base.kind(),
        "arrow_function" | "function_expression" | "function"
    )
}

fn detect_single_param_rename(ctx: &WalkCtx, base: Node, head: Node) -> Option<ParamRename> {
    let base_name = sole_bare_param_name(base, ctx.base_src)?;
    let head_name = sole_bare_param_name(head, ctx.head_src)?;
    if base_name == head_name {
        return None;
    }
    Some(ParamRename {
        head_name,
        base_name,
    })
}

/// Returns the parameter identifier text when the callable's parameter
/// list is exactly one bare identifier (no destructure, default, or rest).
fn sole_bare_param_name(callable: Node, src: &str) -> Option<String> {
    let parameter = sole_parameter_node(callable)?;
    if parameter.kind() != "identifier" {
        return None;
    }
    Some(node_text(parameter, src))
}

fn sole_parameter_node(callable: Node) -> Option<Node> {
    // arrow_function: parameter is either the `parameter` field (single
    // bare identifier, no parens) or inside `parameters` field.
    if let Some(parameter) = callable.child_by_field_name("parameter") {
        return Some(parameter);
    }
    let parameters = callable.child_by_field_name("parameters")?;
    if parameters.kind() != "formal_parameters" {
        return None;
    }
    let named: Vec<Node> = raw_comparable_children(parameters)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    if named.len() != 1 {
        return None;
    }
    Some(named[0])
}

/// Conservatively rejects the rename when the head body mentions the base
/// param name, or the base body mentions the head param name. Either form
/// would risk silently re-interpreting a closure reference as the renamed
/// parameter.
fn shadowing_risk(ctx: &WalkCtx, base: Node, head: Node, rename: &ParamRename) -> bool {
    let Some(head_body) = callable_body(head) else {
        return false;
    };
    let Some(base_body) = callable_body(base) else {
        return false;
    };
    body_has_identifier(head_body, ctx.head_src, &rename.base_name)
        || body_has_identifier(base_body, ctx.base_src, &rename.head_name)
}

fn callable_body(callable: Node) -> Option<Node> {
    callable.child_by_field_name("body")
}

fn body_has_identifier(node: Node, src: &str, name: &str) -> bool {
    if node.kind() == "identifier" && node_text(node, src) == name {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if body_has_identifier(child, src, name) {
            return true;
        }
    }
    false
}
