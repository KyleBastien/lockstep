//! Equivalence rule for the "extract narrowed cache into a const local" pattern.
//!
//! Gated on `allow_non_null_alias_local`. TypeScript discards a narrowing of
//! `this._cache: T | null` across `await`, method calls, or any code that
//! could mutate the field. The canonical TS workaround is:
//!
//! ```text
//! if (!this._cache) { ... return; }
//! const local = this._cache;
//! local.foo(); // narrowing preserved on `local`
//! ```
//!
//! Base JavaScript reuses the original variable directly. This rule
//! recognizes the head `const LOCAL = CACHE;` extraction that appears after a
//! null guard for `CACHE`, drops the declaration from the head child list,
//! and registers a scope-local alias such that subsequent head `LOCAL`
//! references compare equal to base `CACHE` references (which may itself be
//! resolved via `allow_closure_cache_field_alias`).
//!
//! Pure type-system artifact — runtime behavior identical because `LOCAL`
//! and `CACHE` reference the same object until `CACHE` is reassigned. The
//! rule refuses to fire if `LOCAL` is reassigned anywhere in the enclosing
//! block.

use tree_sitter::Node;

use crate::node_utils::{first_named_child, node_text, raw_comparable_children};
use crate::walk::{NonNullAliasLocal, WalkCtx};

/// Composable variant: detects `const LOCAL = CACHE;` extractions in the head
/// block, validates a preceding null guard for `CACHE` and that `LOCAL` is
/// read-only in the block, then registers each alias onto `child_ctx`.
/// Returns `true` when at least one alias was registered.
pub(super) fn apply_non_null_alias_local(
    child_ctx: &mut WalkCtx,
    base: Node,
    head: Node,
) -> bool {
    if !child_ctx.allow_non_null_alias_local {
        return false;
    }
    if base.kind() != "statement_block" || head.kind() != "statement_block" {
        return false;
    }
    let head_src = child_ctx.head_src;
    let head_stmts: Vec<Node> = raw_comparable_children(head)
        .into_iter()
        .filter(|n| n.is_named())
        .collect();
    let mut applied = false;
    for (i, stmt) in head_stmts.iter().enumerate() {
        let Some(extracted) = extract_const_local_assignment(*stmt, head_src) else {
            continue;
        };
        if !preceded_by_null_guard(&head_stmts[..i], head_src, &extracted.cache_compact_text) {
            continue;
        }
        if local_is_reassigned(head, head_src, &extracted.local_name) {
            continue;
        }
        child_ctx.ignored_head_starts.push(stmt.start_byte());
        child_ctx.non_null_aliases.push(NonNullAliasLocal {
            head_local: extracted.local_name,
            base_target_text: extracted.cache_compact_text,
            head_this_property: extracted.cache_this_property,
        });
        applied = true;
    }
    applied
}

/// Leaf-pair resolver: head identifier matches a registered alias and base
/// matches the registered cache expression (directly or via a cache_alias
/// substitution).
pub(super) fn is_non_null_alias_pair(ctx: &WalkCtx, base: Node, head: Node) -> bool {
    if head.kind() != "identifier" {
        return false;
    }
    let head_text = node_text(head, ctx.head_src);
    let Some(alias) = ctx
        .non_null_aliases
        .iter()
        .find(|a| a.head_local == head_text)
    else {
        return false;
    };
    base_matches_alias(ctx, base, alias)
}

fn base_matches_alias(ctx: &WalkCtx, base: Node, alias: &NonNullAliasLocal) -> bool {
    match base.kind() {
        "identifier" => {
            let base_text = node_text(base, ctx.base_src);
            if base_text == alias.base_target_text {
                return true;
            }
            // Bare-identifier ↔ `this.PROP` substitution via cache_alias.
            if let Some(prop) = &alias.head_this_property {
                if ctx
                    .aliases
                    .iter()
                    .any(|a| a.base_name == base_text && a.head_property == *prop)
                {
                    return true;
                }
            }
            false
        }
        "member_expression" => {
            if member_compact_text(base, ctx.base_src) == alias.base_target_text {
                return true;
            }
            false
        }
        _ => false,
    }
}

struct Extracted {
    local_name: String,
    cache_compact_text: String,
    cache_this_property: Option<String>,
}

/// Matches `const LOCAL = CACHE;` where CACHE is `identifier` or
/// `this.PROP`. Returns the extracted parts. `let` is also accepted.
fn extract_const_local_assignment(stmt: Node, src: &str) -> Option<Extracted> {
    let declarator = sole_variable_declarator(stmt)?;
    let name = declarator.child_by_field_name("name")?;
    if name.kind() != "identifier" {
        return None;
    }
    let value = unwrap_parens(declarator.child_by_field_name("value")?);
    let (cache_compact_text, cache_this_property) = extract_cache_form(value, src)?;
    Some(Extracted {
        local_name: node_text(name, src),
        cache_compact_text,
        cache_this_property,
    })
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

fn extract_cache_form(value: Node, src: &str) -> Option<(String, Option<String>)> {
    match value.kind() {
        "identifier" => Some((node_text(value, src), None)),
        "member_expression" => {
            let prop = this_property_name(value, src)?;
            Some((format!("this.{prop}"), Some(prop)))
        }
        _ => None,
    }
}

fn preceded_by_null_guard(prior_stmts: &[Node], src: &str, cache_compact_text: &str) -> bool {
    prior_stmts
        .iter()
        .any(|s| is_null_guard_for(*s, src, cache_compact_text))
}

/// Returns `true` for `if (!CACHE) { ...; return X; }` or `if (!CACHE) { ...;
/// throw X; }`. Accepts any guard body content as long as one terminating
/// statement (return or throw) appears.
fn is_null_guard_for(stmt: Node, src: &str, cache_compact_text: &str) -> bool {
    if stmt.kind() != "if_statement" {
        return false;
    }
    let Some(condition) = stmt.child_by_field_name("condition") else {
        return false;
    };
    let condition = unwrap_parens(condition);
    if condition.kind() != "unary_expression" {
        return false;
    }
    if !unary_is_negation(condition, src) {
        return false;
    }
    let arg = unwrap_parens(unary_argument(condition).unwrap_or(condition));
    if compact_expr_text(arg, src) != cache_compact_text {
        return false;
    }
    let Some(consequence) = stmt.child_by_field_name("consequence") else {
        return false;
    };
    body_terminates(consequence)
}

fn body_terminates(body: Node) -> bool {
    match body.kind() {
        "return_statement" | "throw_statement" => true,
        "statement_block" => raw_comparable_children(body)
            .into_iter()
            .filter(|n| n.is_named())
            .any(|n| matches!(n.kind(), "return_statement" | "throw_statement")),
        _ => false,
    }
}

/// Searches the entire block subtree for any reassignment of `local_name`.
fn local_is_reassigned(root: Node, src: &str, local_name: &str) -> bool {
    if is_local_assignment_target(root, src, local_name) {
        return true;
    }
    let mut cursor = root.walk();
    let children: Vec<Node> = root.children(&mut cursor).collect();
    children
        .into_iter()
        .any(|child| local_is_reassigned(child, src, local_name))
}

fn is_local_assignment_target(node: Node, src: &str, local_name: &str) -> bool {
    match node.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            let Some(left) = node.child_by_field_name("left") else {
                return false;
            };
            left.kind() == "identifier" && node_text(left, src) == local_name
        }
        "update_expression" => {
            let Some(arg) = node
                .child_by_field_name("argument")
                .or_else(|| first_named_child(node))
            else {
                return false;
            };
            arg.kind() == "identifier" && node_text(arg, src) == local_name
        }
        _ => false,
    }
}

fn this_property_name(node: Node, src: &str) -> Option<String> {
    if node.kind() != "member_expression" {
        return None;
    }
    let object = node.child_by_field_name("object")?;
    if node_text(object, src) != "this" {
        return None;
    }
    node.child_by_field_name("property")
        .map(|property| node_text(property, src))
}

fn member_compact_text(node: Node, src: &str) -> String {
    node_text(node, src)
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn compact_expr_text(node: Node, src: &str) -> String {
    node_text(node, src)
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn unary_is_negation(node: Node, src: &str) -> bool {
    if let Some(op) = node.child_by_field_name("operator") {
        return node_text(op, src) == "!";
    }
    let mut cursor = node.walk();
    let children: Vec<Node> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .filter(|c| !c.is_named())
        .any(|c| node_text(c, src) == "!")
}

fn unary_argument(node: Node) -> Option<Node> {
    if let Some(arg) = node.child_by_field_name("argument") {
        return Some(arg);
    }
    first_named_child(node)
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
