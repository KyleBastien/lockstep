//! Equivalence rule for narrowing-helper-wrapped local extraction.
//!
//! Gated on `allow_helper_call_site_substitution` (typically cascaded from
//! `allow_pure_narrowing_helper`). Extends the narrowing-helper allowlist
//! from "declaration filtered + inline call site recognized" to "extracted
//! local reads recognized."
//!
//! Head shape (any one of):
//!
//! ```text
//! const LOCAL = HELPER(EXPR) ?? DEFAULT;          // nullish-default form
//! const LOCAL = HELPER(EXPR) ? EXPR : DEFAULT;    // type-predicate ternary form
//! ```
//!
//! where `HELPER` is configured in `narrowing_helpers` AND declared at the
//! head top level (so it survives `register_narrowing_helper_declarations`),
//! and `DEFAULT` is one of the safe defaults recognized by
//! [`crate::pure_narrowing_helper::is_safe_default`]. Subsequent reads of
//! `LOCAL` in the enclosing block compare equal to base nodes whose compact
//! text matches `EXPR` *modulo optional-chain markers* (`?.` and `?[`).
//! Head can defensively add `?.` against a base direct access (or remove
//! `?.` against a base optional access) — the `?? DEFAULT` already
//! normalizes the value, so the runtime-edge divergence sits in the same
//! envelope as the rest of the narrowing-helper rules.
//!
//! Composes with the rest of the walker — Rule 2 (destructure-then-narrow)
//! reuses the same per-statement recognition. Aliases are scope-bounded by
//! the `statement_block` the rule is applied on.
//!
//! Out of scope (v1):
//! - Multi-declarator `const a = ..., b = ...;` declarations.
//! - Aliasing across reassignment (`let LOCAL = ...; ... LOCAL = OTHER;`).
//! - `LOCAL` exported / closed over by an inner function (the in-block
//!   read-only check only catches reassignments and shadowing on the same
//!   block; transitive closures are unsupported).

use tree_sitter::Node;

use crate::node_utils::{compact_node_text, first_named_child, node_text, raw_comparable_children};
use crate::pure_narrowing_helper::{is_safe_default, recognized_helper_name, sole_call_argument};
use crate::walk::{HelperCallSiteAlias, WalkCtx};

/// Leaf-pair pre-empt: head identifier matches a registered alias and the
/// base node's compact text matches the alias's `base_expr_text` modulo
/// optional-chain markers. The `?.` tolerance accepts a head helper arg
/// that defensively adds optional chaining against a base reference that
/// directly accesses the property (and vice versa) — the helper's
/// `?? DEFAULT` already guarantees a defined value at the use site, so
/// the divergence at the runtime edge sits in the same envelope as the
/// rest of the rule.
pub(super) fn is_helper_call_site_alias_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if !ctx.allow_helper_call_site_substitution {
        return false;
    }
    if head.kind() != "identifier" {
        return false;
    }
    let head_text = node_text(head, ctx.head_src);
    let Some(alias) = ctx
        .helper_call_site_aliases
        .iter()
        .find(|a| a.head_local == head_text)
    else {
        return false;
    };
    equal_modulo_optional_chain(
        &compact_node_text(base, ctx.base_src),
        &alias.base_expr_text,
    )
}

/// String equality modulo optional-chain markers: `?` characters that
/// immediately precede `.` or `[` are stripped from both sides before
/// comparison. Leaves other `?` characters alone (e.g. bare `?` inside a
/// ternary), though compact text from member-/subscript-/identifier-/
/// call-expression nodes is unlikely to contain those.
fn equal_modulo_optional_chain(a: &str, b: &str) -> bool {
    strip_optional_chain_markers(a) == strip_optional_chain_markers(b)
}

fn strip_optional_chain_markers(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'?' && i + 1 < bytes.len() && (bytes[i + 1] == b'.' || bytes[i + 1] == b'[')
        {
            i += 1;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Composable block-strip. Scans `head` for `const LOCAL = HELPER(EXPR) ??
/// DEFAULT;` (or the type-predicate ternary form) declarations whose
/// `HELPER` is a recognized narrowing helper. For each match, strips the
/// declaration from the comparable child set and registers a scope-local
/// alias mapping `LOCAL` → compact text of `EXPR`. Returns `true` when at
/// least one alias was registered.
pub(super) fn apply_helper_call_site_substitution(
    child_ctx: &mut WalkCtx,
    base: Node,
    head: Node,
) -> bool {
    if !child_ctx.allow_helper_call_site_substitution {
        return false;
    }
    if child_ctx.recognized_narrowing_helpers.is_empty() {
        return false;
    }
    if base.kind() != "statement_block" || head.kind() != "statement_block" {
        return false;
    }
    let head_stmts: Vec<Node> = raw_comparable_children(head)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    let mut applied = false;
    for stmt in &head_stmts {
        let Some(extract) = extract_helper_call_site(*stmt, child_ctx) else {
            continue;
        };
        if !local_is_read_only_in_stmts(&head_stmts, child_ctx.head_src, &extract.local_name) {
            continue;
        }
        child_ctx.ignored_head_starts.push(stmt.start_byte());
        child_ctx
            .helper_call_site_aliases
            .push(HelperCallSiteAlias {
                head_local: extract.local_name,
                base_expr_text: extract.base_expr_text,
            });
        applied = true;
    }
    applied
}

/// Inspects a single statement. Returns `Some(Extract)` when the statement
/// is a single-declarator `const LOCAL = ...;` whose value is one of the
/// recognized narrowing-helper shapes.
pub(super) fn extract_helper_call_site(stmt: Node, ctx: &WalkCtx) -> Option<Extract> {
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
    let decl = declarators[0];
    extract_from_declarator(decl, ctx)
}

pub(super) struct Extract {
    pub(super) local_name: String,
    pub(super) base_expr_text: String,
}

fn extract_from_declarator(decl: Node, ctx: &WalkCtx) -> Option<Extract> {
    let name = decl.child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    let local_name = node_text(name, ctx.head_src);
    let value = unwrap_parens(decl.child_by_field_name("value")?);
    let expr_node =
        extract_from_nullish_default(value, ctx).or_else(|| extract_from_predicate(value, ctx))?;
    Some(Extract {
        local_name,
        base_expr_text: compact_node_text(expr_node, ctx.head_src),
    })
}

/// `HELPER(EXPR) ?? DEFAULT` — recognizes the value-narrowing form. Returns
/// the inner `EXPR` node so the caller can capture its compact text.
fn extract_from_nullish_default<'a>(value: Node<'a>, ctx: &WalkCtx) -> Option<Node<'a>> {
    let (call, default) = unwrap_nullish_default(value, ctx.head_src)?;
    if !is_safe_default(default, ctx.head_src) {
        return None;
    }
    let _ = recognized_helper_name(ctx, call)?;
    Some(unwrap_parens(sole_call_argument(call)?))
}

fn unwrap_nullish_default<'a>(value: Node<'a>, src: &str) -> Option<(Node<'a>, Node<'a>)> {
    if value.kind() != "binary_expression" {
        return None;
    }
    let op = value.child_by_field_name("operator")?;
    if node_text(op, src) != "??" {
        return None;
    }
    let left = unwrap_parens(value.child_by_field_name("left")?);
    let right = unwrap_parens(value.child_by_field_name("right")?);
    if left.kind() != "call_expression" {
        return None;
    }
    Some((left, right))
}

/// `HELPER(EXPR) ? EXPR : DEFAULT` — type-predicate ternary form. Both arms
/// of the ternary must agree on `EXPR`; the alternative must be a safe
/// default. Returns the inner `EXPR` node.
fn extract_from_predicate<'a>(value: Node<'a>, ctx: &WalkCtx) -> Option<Node<'a>> {
    let parts = unwrap_predicate_ternary(value)?;
    let _ = recognized_helper_name(ctx, parts.condition)?;
    let arg = unwrap_parens(sole_call_argument(parts.condition)?);
    if !predicate_arms_agree(arg, parts.consequence, parts.alternative, ctx.head_src) {
        return None;
    }
    Some(arg)
}

struct PredicateTernary<'a> {
    condition: Node<'a>,
    consequence: Node<'a>,
    alternative: Node<'a>,
}

fn unwrap_predicate_ternary<'a>(value: Node<'a>) -> Option<PredicateTernary<'a>> {
    if value.kind() != "ternary_expression" {
        return None;
    }
    let condition = unwrap_parens(value.child_by_field_name("condition")?);
    if condition.kind() != "call_expression" {
        return None;
    }
    let consequence = unwrap_parens(value.child_by_field_name("consequence")?);
    let alternative = unwrap_parens(value.child_by_field_name("alternative")?);
    Some(PredicateTernary {
        condition,
        consequence,
        alternative,
    })
}

fn predicate_arms_agree(arg: Node, consequence: Node, alternative: Node, src: &str) -> bool {
    if compact_node_text(arg, src) != compact_node_text(consequence, src) {
        return false;
    }
    is_safe_default(alternative, src)
}

/// Conservatively rejects when the local appears on the left-hand side of
/// any assignment expression in the surrounding statement set. Reads are
/// allowed (any number of times); a single LHS occurrence breaks
/// equivalence and re-flags the divergence.
fn local_is_read_only_in_stmts(stmts: &[Node], src: &str, local: &str) -> bool {
    for stmt in stmts {
        if local_is_lhs_target(*stmt, src, local) {
            return false;
        }
    }
    true
}

fn local_is_lhs_target(node: Node, src: &str, local: &str) -> bool {
    if node.kind() == "assignment_expression" {
        if let Some(left) = node.child_by_field_name("left") {
            if left.kind() == "identifier" && node_text(left, src) == local {
                return true;
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if local_is_lhs_target(child, src, local) {
            return true;
        }
    }
    false
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
