//! Equivalence rule for zero-argument config-reader narrowing helpers
//! (Gap 2B).
//!
//! Gated on the `narrowing_helpers_aliases` config table — a map of helper
//! name → base path text. Without the table the rule is a no-op.
//!
//! Head pattern (inside a `statement_block`):
//!
//! ```text
//! const LOCAL = HELPER();
//! ```
//!
//! When `HELPER` appears in `narrowing_helpers_aliases` with mapped path
//! `BASE_PATH`, the declaration is stripped from head and an alias
//! `LOCAL` ↔ `BASE_PATH` is registered. Subsequent head reads of
//! `LOCAL[.X.Y…]` compare equal to base reads of `BASE_PATH[.X.Y…]`. The
//! `BASE_PATH` text usually contains an optional-chain marker
//! (`config.pp_config?`) — this is preserved so head's removal of the
//! optional chain composes with the alias.
//!
//! There is no corresponding base statement to strip — the base reads the
//! configured path inline at each use site.

use tree_sitter::Node;

use crate::node_utils::{compact_node_text, node_text, raw_comparable_children};
use crate::walk::{HelperZeroArgAlias, WalkCtx};

/// Composable block-strip. Scans `head` for `const LOCAL = HELPER();`
/// declarations whose `HELPER` is registered in
/// `narrowing_helpers_aliases`. Returns `true` when at least one alias
/// was registered.
pub(super) fn apply_helper_zero_arg_alias(
    child_ctx: &mut WalkCtx,
    _base: Node,
    head: Node,
) -> bool {
    if child_ctx.narrowing_helpers_aliases.is_empty() {
        return false;
    }
    if head.kind() != "statement_block" {
        return false;
    }
    let head_stmts: Vec<Node> = raw_comparable_children(head)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    let mut applied = false;
    for stmt in &head_stmts {
        let Some((local, helper)) = extract_zero_arg_helper(*stmt, child_ctx.head_src) else {
            continue;
        };
        let Some(base_path) = child_ctx.narrowing_helpers_aliases.get(&helper).cloned() else {
            continue;
        };
        child_ctx.ignored_head_starts.push(stmt.start_byte());
        child_ctx.helper_zero_arg_aliases.push(HelperZeroArgAlias {
            head_local: local,
            base_path,
        });
        applied = true;
    }
    applied
}

/// Leaf-pair pre-empt: head node's compact text begins with a registered
/// alias local; substituting the local with `base_path` yields a string
/// equal to the base node's compact text.
pub(super) fn is_helper_zero_arg_alias_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if ctx.helper_zero_arg_aliases.is_empty() {
        return false;
    }
    if !is_alias_capable_kind(head.kind()) {
        return false;
    }
    let head_text = compact_node_text(head, ctx.head_src);
    let base_text = compact_node_text(base, ctx.base_src);
    for alias in &ctx.helper_zero_arg_aliases {
        if !head_text_begins_with_alias(&head_text, &alias.head_local) {
            continue;
        }
        let after = &head_text[alias.head_local.len()..];
        let substituted = format!("{}{}", alias.base_path, after);
        if substituted == base_text {
            return true;
        }
    }
    false
}

fn is_alias_capable_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier" | "member_expression" | "subscript_expression" | "call_expression"
    )
}

/// True when `text == local` (bare reference) or `text` is `local`
/// immediately followed by a property accessor character (`.`, `?`, `[`).
fn head_text_begins_with_alias(text: &str, local: &str) -> bool {
    if !text.starts_with(local) {
        return false;
    }
    let after = &text[local.len()..];
    after.is_empty() || after.starts_with('.') || after.starts_with('?') || after.starts_with('[')
}

fn extract_zero_arg_helper(stmt: Node, src: &str) -> Option<(String, String)> {
    let decl = sole_variable_declarator(stmt)?;
    let name = decl.child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    let value = decl.child_by_field_name("value")?;
    let helper = zero_arg_helper_callee(value)?;
    Some((node_text(name, src), node_text(helper, src)))
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

fn zero_arg_helper_callee(value: Node) -> Option<Node> {
    if value.kind() != "call_expression" {
        return None;
    }
    let callee = value.child_by_field_name("function")?;
    if callee.kind() != "identifier" {
        return None;
    }
    if !call_arguments_are_empty(value) {
        return None;
    }
    Some(callee)
}

fn call_arguments_are_empty(call: Node) -> bool {
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    raw_comparable_children(arguments)
        .into_iter()
        .filter(|n| n.is_named())
        .count()
        == 0
}
